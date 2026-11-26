use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, ExitStatus, Stdio},
};

const READ_SIZE: usize = 8 * 1024;
const READS_PER_POLL: usize = 8;

pub struct Pty {
    master: File,
    child: Child,
    input: VecDeque<u8>,
    status: Option<ExitStatus>,
}

impl Pty {
    pub fn spawn(
        mut command: Command,
        mode: &libc::termios,
        rows: u16,
        cols: u16,
    ) -> io::Result<Self> {
        let (master, slave) = open_pair()?;
        // SAFETY: The owned slave descriptor and the caller's termios are valid for the call.
        syscall(unsafe { libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, mode) })?;
        set_size(&master, rows, cols)?;

        command
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave))
            .env("TERM", "xterm-256color");
        // SAFETY: This callback runs after stdio is assigned. It invokes only the session and
        // terminal syscalls and reads errno on failure, without allocating or locking.
        unsafe {
            command.pre_exec(|| {
                syscall(libc::setsid())?;
                syscall(libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY, 0))?;
                Ok(())
            });
        }
        let child = command.spawn()?;

        Ok(Self {
            master,
            child,
            input: VecDeque::new(),
            status: None,
        })
    }

    pub fn resize(&self, rows: u16, cols: u16) -> io::Result<()> {
        set_size(&self.master, rows, cols)
    }

    pub fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.status.is_none() {
            self.input.extend(bytes);
            self.flush_input()?;
        }
        Ok(())
    }

    pub fn poll(&mut self, output: &mut Vec<u8>) -> io::Result<Option<ExitStatus>> {
        if self.status.is_some() {
            return Ok(self.status);
        }

        self.flush_input()?;
        // Check for exit before reading so the last write cannot race with an empty read.
        let exited = self.child_exited()?;
        if exited {
            // Stop any group members still producing output after the command exits.
            self.kill_group();
        }
        if self.drain_output(output)? && exited {
            self.status = Some(self.child.wait()?);
        }
        Ok(self.status)
    }

    fn flush_input(&mut self) -> io::Result<()> {
        let (bytes, _) = self.input.as_slices();
        if bytes.is_empty() {
            return Ok(());
        }

        match self.master.write(&bytes[..bytes.len().min(READ_SIZE)]) {
            Ok(written) => {
                self.input.drain(..written);
            }
            Err(error) if error.raw_os_error() == Some(libc::EIO) => self.input.clear(),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn drain_output(&mut self, output: &mut Vec<u8>) -> io::Result<bool> {
        let mut buffer = [0; READ_SIZE];
        for _ in 0..READS_PER_POLL {
            match self.master.read(&mut buffer) {
                Ok(0) => return Ok(true),
                Ok(count) => output.extend_from_slice(&buffer[..count]),
                // Linux PTY masters return EIO after the last slave descriptor closes.
                Err(error) if error.raw_os_error() == Some(libc::EIO) => return Ok(true),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(true),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    fn child_exited(&self) -> io::Result<bool> {
        // Linux leaves si_pid at zero when there is no exit event with WNOHANG.
        // SAFETY: Zero is a valid initial state for the waitid result buffer.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // Keep the child waitable so its PID stays reserved through process-group cleanup.
        // SAFETY: The child ID identifies a direct child and info is writable.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        match syscall(result) {
            // SAFETY: waitid succeeded and si_pid is valid for its SIGCHLD result or zero.
            Ok(_) => Ok(unsafe { info.si_pid() } != 0),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn kill_group(&self) {
        // SAFETY: setsid made the child's still-reserved PID its process-group ID.
        unsafe { libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL) };
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        if self.status.is_none() {
            self.kill_group();
            let _ = self.child.wait();
        }
    }
}

fn open_pair() -> io::Result<(File, File)> {
    // Set CLOEXEC atomically, as background read workers can spawn other commands.
    // SAFETY: posix_openpt takes open flags and returns a uniquely owned descriptor on success.
    let master = syscall(unsafe {
        libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC)
    })?;
    // SAFETY: The successful open returned a descriptor owned by this function.
    let master = unsafe { File::from_raw_fd(master) };
    // SAFETY: The owned descriptor refers to a live PTY master.
    unsafe {
        syscall(libc::grantpt(master.as_raw_fd()))?;
        syscall(libc::unlockpt(master.as_raw_fd()))?;
    }
    // SAFETY: TIOCGPTPEER takes open flags and returns an owned descriptor for the slave.
    let slave = syscall(unsafe {
        libc::ioctl(
            master.as_raw_fd(),
            libc::TIOCGPTPEER,
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    })?;
    // SAFETY: The successful ioctl returned a descriptor owned by this function.
    Ok((master, unsafe { File::from_raw_fd(slave) }))
}

fn set_size(master: &File, rows: u16, cols: u16) -> io::Result<()> {
    let size = libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ reads the winsize through a valid pointer during the ioctl.
    syscall(unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &raw const size) })?;
    Ok(())
}

fn syscall(result: libc::c_int) -> io::Result<libc::c_int> {
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        mem::MaybeUninit,
        os::unix::process::ExitStatusExt,
        thread,
        time::{Duration, Instant},
    };

    fn shell(script: &str) -> Pty {
        let (_master, slave) = open_pair().unwrap();
        let mut mode = MaybeUninit::uninit();
        // SAFETY: tcgetattr initializes the output termios on success.
        syscall(unsafe { libc::tcgetattr(slave.as_raw_fd(), mode.as_mut_ptr()) }).unwrap();
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        // SAFETY: tcgetattr succeeded and initialized mode.
        Pty::spawn(command, &unsafe { mode.assume_init() }, 12, 55).unwrap()
    }

    fn until<T>(mut poll: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(value) = poll() {
                return value;
            }
            assert!(Instant::now() < deadline, "PTY poll timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn supports_a_cooked_prompt_with_controlling_terminal_and_resize() {
        let mut pty = shell(
            r#"test -t 0 && test -t 1 && test -t 2 && test "$TERM" = xterm-256color || exit 1
stty size </dev/tty || exit 2
printf 'answer? ' >&2
IFS= read -r answer
stty size </dev/tty || exit 3
printf 'answer=%s\n' "$answer"
exit 7"#,
        );
        let mut output = Vec::new();
        until(|| {
            assert!(pty.poll(&mut output).unwrap().is_none());
            output.ends_with(b"answer? ").then_some(())
        });
        pty.resize(20, 70).unwrap();
        pty.send(b"yes\r").unwrap();
        assert_eq!(until(|| pty.poll(&mut output).unwrap()).code(), Some(7));
        assert_eq!(output, b"12 55\r\nanswer? yes\r\n20 70\r\nanswer=yes\r\n");
    }

    #[test]
    fn delivers_terminal_interrupt_to_the_foreground_process() {
        let mut pty = shell("printf ready && exec sleep 60");
        let mut output = Vec::new();
        until(|| {
            assert!(pty.poll(&mut output).unwrap().is_none());
            output.ends_with(b"ready").then_some(())
        });
        pty.send(b"\x03").unwrap();
        assert_eq!(
            until(|| pty.poll(&mut output).unwrap()).signal(),
            Some(libc::SIGINT)
        );
    }

    #[test]
    fn drains_large_output_before_reporting_exit() {
        let mut pty = shell("head -c 262144 /dev/zero && printf end");
        let mut output = Vec::new();
        let status = until(|| {
            let previous = output.len();
            let status = pty.poll(&mut output).unwrap();
            assert!(output.len() - previous <= READ_SIZE * READS_PER_POLL);
            status
        });
        assert!(status.success());
        assert_eq!(output.len(), 262_147);
        assert!(output.ends_with(b"end"));
    }

    #[test]
    fn sends_input_across_multiple_nonblocking_writes() {
        let mut pty = shell("stty raw -echo && printf ready && head -c 32768");
        let mut output = Vec::new();
        until(|| {
            assert!(pty.poll(&mut output).unwrap().is_none());
            output.ends_with(b"ready").then_some(())
        });
        output.clear();
        let input: Vec<_> = (b'a'..=b'z').cycle().take(32768).collect();
        pty.send(&input).unwrap();
        assert!(until(|| pty.poll(&mut output).unwrap()).success());
        assert_eq!(output, input);
    }

    #[test]
    fn dropping_a_running_session_reaps_the_child() {
        let pty = shell("exec sleep 60");
        let pid = pty.child.id() as libc::pid_t;
        drop(pty);
        // SAFETY: WNOHANG makes waitpid nonblocking and a null status pointer is allowed.
        let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
        assert_eq!(
            syscall(result).unwrap_err().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}

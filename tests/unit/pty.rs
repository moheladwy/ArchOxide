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

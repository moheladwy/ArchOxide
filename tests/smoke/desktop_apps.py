"""Exercise desktop launching with a controlling PTY and a fixture gio."""

from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import pty
import select
import subprocess
import tempfile
import termios
import time
import unittest


def attach_controlling_terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


class Terminal:
    def __init__(self, master):
        self.master = master
        self.output = bytearray()

    def send(self, data):
        os.write(self.master, data)

    def wait_for(self, text):
        deadline = time.monotonic() + 5
        while text not in self.output:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.master], [], [], remaining)[0]:
                raise AssertionError(f"Missing {text!r}: {self.output[-750:]!r}")
            chunk = os.read(self.master, 8192)
            if not chunk:
                raise AssertionError(f"ArchOxide exited before rendering {text!r}")
            self.output.extend(chunk)


@contextmanager
def archoxide(root, gio_script, *, enhanced=False):
    applications = root / "data" / "applications"
    applications.mkdir(parents=True)
    (applications / "fixture.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=Launch fixture\nExec=/bin/true\n"
    )
    gio = root / "gio"
    gio.write_text("#!/bin/sh\n" + gio_script)
    gio.chmod(0o755)
    env = dict(
        os.environ,
        PATH=str(root),
        TERM="xterm-256color",
        XDG_CONFIG_HOME=str(root / "config"),
        XDG_DATA_HOME=str(root / "data"),
        XDG_DATA_DIRS=str(root / "system"),
        LAUNCH_RESULT=str(root / "launch-result"),
    )
    master, slave = pty.openpty()
    try:
        termios.tcsetwinsize(slave, (24, 110))
        original_mode = termios.tcgetattr(slave)
        with subprocess.Popen(
            ["target/debug/archoxide"],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            preexec_fn=attach_controlling_terminal,
        ) as child:
            terminal = Terminal(master)
            try:
                if enhanced:
                    terminal.wait_for(b"\x1b[?u\x1b[c")
                    terminal.send(b"\x1b[?0u\x1b[?1;2c")
                    terminal.wait_for(b"\x1b[>11u")
                terminal.wait_for(b"Filter installed")
                terminal.send(b"\x1b[32u\x1b[111u" if enhanced else b" o")
                terminal.wait_for(b"Launch fixture")
                yield terminal
                terminal.send(b"\x1b[32u\x1b[113u" if enhanced else b" q")
                assert child.wait(timeout=5) == 0
                assert termios.tcgetattr(slave) == original_mode
                if enhanced:
                    terminal.wait_for(b"\x1b[<1u")
            finally:
                if child.poll() is None:
                    child.kill()
    finally:
        os.close(master)
        os.close(slave)


class DesktopAppsSmoke(unittest.TestCase):
    def test_launch_cannot_access_the_controlling_terminal(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with archoxide(
                root,
                """if ( : </dev/tty ) 2>/dev/null; then
    printf attached > "$LAUNCH_RESULT"
else
    printf detached > "$LAUNCH_RESULT"
fi
""",
            ) as terminal:
                terminal.send(b"\r")
                terminal.wait_for(b"ed Launch fixture.")
                self.assertEqual((root / "launch-result").read_text(), "detached")

    def test_failed_launch_displays_the_gio_diagnostic(self):
        with tempfile.TemporaryDirectory() as temp:
            desktop_path = "/long-application-directory" * 3 + "/fixture.desktop"
            with archoxide(
                Path(temp),
                f"printf 'gio: Unable to launch application {desktop_path}: "
                "Unable to find terminal required for application\\n' >&2\nexit 1\n",
            ) as terminal:
                terminal.send(b"\r")
                terminal.wait_for(b"required for application")

    def test_enhanced_terminal_ignores_repeat_events_after_launch(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with archoxide(root, 'printf x >> "$LAUNCH_RESULT"\n', enhanced=True) as terminal:
                terminal.send(b"\x1b[13u")
                terminal.wait_for(b"ed Launch fixture.")
                terminal.send(b"\x1b[13;1:2u\x1b[13;1:3u")
                time.sleep(0.2)
                self.assertEqual((root / "launch-result").read_text(), "x")
                terminal.output.clear()
                terminal.send(b"\x1b[13u")
                terminal.wait_for(b"ed Launch fixture.")
                self.assertEqual((root / "launch-result").read_text(), "xx")


if __name__ == "__main__":
    unittest.main()

//! Single-instance control channel. On Linux a Unix socket lets compositor keybinds
//! (`aimtrainer toggle`) show or hide the overlay, which is the only reliable way to get
//! a global shortcut on Wayland.

use bevy::prelude::Resource;
use std::sync::Mutex;
use std::sync::mpsc::Receiver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Show,
    Hide,
    /// Show the overlay and start a queue warm-up of this many minutes; without minutes,
    /// of the personal dose (when on and known) or the Settings length.
    Warmup(Option<u32>),
    /// Show the overlay, play one buy-phase micro drill, then hide again.
    Micro,
}

impl Command {
    pub fn parse(s: &str) -> Option<Self> {
        let mut words = s.split_whitespace();
        let cmd = match words.next()? {
            "toggle" => Self::Toggle,
            "show" => Self::Show,
            "hide" => Self::Hide,
            "micro" => Self::Micro,
            "warmup" => {
                let minutes = match words.next() {
                    Some(m) => Some(m.parse::<u32>().ok().filter(|m| (1..=30).contains(m))?),
                    None => None,
                };
                Self::Warmup(minutes)
            }
            _ => return None,
        };
        words.next().is_none().then_some(cmd)
    }

    #[cfg(unix)]
    fn encode(self) -> String {
        match self {
            Self::Toggle => "toggle".into(),
            Self::Show => "show".into(),
            Self::Hide => "hide".into(),
            Self::Micro => "micro".into(),
            Self::Warmup(Some(m)) => format!("warmup {m}"),
            Self::Warmup(None) => "warmup".into(),
        }
    }
}

#[derive(Resource)]
pub struct IpcReceiver(pub Mutex<Receiver<Command>>);

#[cfg(unix)]
mod imp {
    use super::Command;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver};

    pub fn socket_path() -> PathBuf {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        dir.join("aimtrainer.sock")
    }

    pub fn send(cmd: Command) -> Result<(), String> {
        send_to(&socket_path(), cmd)
    }

    pub fn send_to(path: &Path, cmd: Command) -> Result<(), String> {
        let mut stream = UnixStream::connect(path)
            .map_err(|e| format!("aimtrainer is not running ({}: {e})", path.display()))?;
        writeln!(stream, "{}", cmd.encode()).map_err(|e| e.to_string())
    }

    pub fn listen() -> Result<Option<Receiver<Command>>, String> {
        listen_at(&socket_path())
    }

    pub fn listen_at(path: &Path) -> Result<Option<Receiver<Command>>, String> {
        if UnixStream::connect(path).is_ok() {
            return Err(
                "another aimtrainer instance is already running; use `aimtrainer toggle`".into(),
            );
        }
        let _ = std::fs::remove_file(path); // stale socket from a crash
        let listener =
            UnixListener::bind(path).map_err(|e| format!("cannot bind {}: {e}", path.display()))?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("aimtrainer-ipc".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        if let Some(cmd) = Command::parse(&line)
                            && tx.send(cmd).is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Some(rx))
    }
}

#[cfg(not(unix))]
mod imp {
    use super::Command;
    use std::sync::mpsc::Receiver;

    pub fn send(_cmd: Command) -> Result<(), String> {
        Err("command-line control is only available on Linux; use the global hotkey".into())
    }

    pub fn listen() -> Result<Option<Receiver<Command>>, String> {
        Ok(None)
    }
}

pub use imp::{listen, send};
#[cfg(all(unix, test))]
use imp::{listen_at, send_to};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands_only() {
        assert_eq!(Command::parse("toggle\n"), Some(Command::Toggle));
        assert_eq!(Command::parse("hide"), Some(Command::Hide));
        assert_eq!(Command::parse("explode"), None);
        assert_eq!(Command::parse("warmup"), Some(Command::Warmup(None)));
        assert_eq!(Command::parse("warmup 3"), Some(Command::Warmup(Some(3))));
        assert_eq!(Command::parse("warmup 0"), None);
        assert_eq!(Command::parse("warmup x"), None);
        assert_eq!(Command::parse("micro"), Some(Command::Micro));
        assert_eq!(Command::parse("toggle now"), None);
    }

    #[cfg(unix)]
    #[test]
    fn toggle_round_trips_and_second_instance_is_refused() {
        let dir = std::env::temp_dir().join(format!("aimtrainer-ipc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("t.sock");
        let rx = listen_at(&sock).unwrap().unwrap();
        send_to(&sock, Command::Toggle).unwrap();
        send_to(&sock, Command::Hide).unwrap();
        let timeout = std::time::Duration::from_secs(2);
        assert_eq!(rx.recv_timeout(timeout), Ok(Command::Toggle));
        assert_eq!(rx.recv_timeout(timeout), Ok(Command::Hide));
        assert!(
            listen_at(&sock).is_err(),
            "a second instance must not start"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn sending_without_a_running_instance_fails_cleanly() {
        let missing = std::env::temp_dir().join("aimtrainer-no-such.sock");
        let err = send_to(&missing, Command::Toggle).unwrap_err();
        assert!(err.contains("not running"), "{err}");
    }
}

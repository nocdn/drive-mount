//! One running copy: the first process listens on a loopback port it records
//! in the app data folder; a later launch asks it to show its window and
//! exits.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

const SHOW: &str = "cloud-drive-mount:show";
const ACK: &str = "cloud-drive-mount:ok";

fn port_file() -> PathBuf {
    crate::paths::app_data_dir().join("instance.port")
}

/// What this launch should do.
pub enum Instance {
    /// No other copy runs: this one owns the listener (none when loopback
    /// is unavailable), which calls the handler passed to [`Instance::serve`]
    /// for each later launch.
    Primary(Option<TcpListener>),
    /// Another copy runs and was asked to show its window.
    Secondary,
}

pub fn acquire() -> Instance {
    if ask_running_copy_to_show() {
        return Instance::Secondary;
    }
    match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => {
            if let Ok(address) = listener.local_addr() {
                let _ = std::fs::create_dir_all(crate::paths::app_data_dir());
                let _ = std::fs::write(port_file(), address.port().to_string());
            }
            Instance::Primary(Some(listener))
        }
        // No loopback: run without the guard rather than not at all.
        Err(_) => Instance::Primary(None),
    }
}

impl Instance {
    /// Calls `on_show` on a background thread whenever another launch asks.
    pub fn serve(listener: TcpListener, on_show: impl Fn() + Send + 'static) {
        std::thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut line = String::new();
                    let mut reader = BufReader::new(&stream);
                    if reader.read_line(&mut line).is_ok() && line.trim() == SHOW {
                        let _ = (&stream).write_all(format!("{ACK}\n").as_bytes());
                        on_show();
                    }
                }
            })
            .ok();
    }
}

fn ask_running_copy_to_show() -> bool {
    let Ok(port) = std::fs::read_to_string(port_file()) else {
        return false;
    };
    let Ok(port) = port.trim().parse::<u16>() else {
        return false;
    };
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(300)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if stream.write_all(format!("{SHOW}\n").as_bytes()).is_err() {
        return false;
    }
    // Something else may have the port now; only our reply counts.
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply).is_ok() && reply.trim() == ACK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_second_launch_asks_the_first_to_show() {
        let _guard = crate::test_support::env_lock();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));

        let Instance::Primary(Some(listener)) = acquire() else {
            panic!("the first launch should own the guard");
        };
        let (shown, seen) = mpsc::channel();
        Instance::serve(listener, move || {
            let _ = shown.send(());
        });
        assert!(matches!(acquire(), Instance::Secondary));
        seen.recv_timeout(Duration::from_secs(2)).unwrap();

        crate::test_support::clear_test_dirs();
    }

    #[test]
    fn a_stale_port_file_does_not_block_launch() {
        let _guard = crate::test_support::env_lock();
        let temp = tempfile::tempdir().unwrap();
        crate::test_support::set_test_dirs(&temp.path().join("app"), &temp.path().join("logs"));
        // A port nobody answers on with our handshake.
        let silent = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        std::fs::create_dir_all(crate::paths::app_data_dir()).unwrap();
        std::fs::write(port_file(), silent.local_addr().unwrap().port().to_string()).unwrap();
        drop(silent);

        assert!(matches!(acquire(), Instance::Primary(_)));
        crate::test_support::clear_test_dirs();
    }
}

//! Only one AirDash runs at a time: a second launch asks the first to show its window and exits.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

/// Next to the OAuth redirect port (53682).
const PORT: u16 = 53681;
const HELLO: &[u8] = b"airdash:show";

pub enum Instance {
    /// This is the only instance; pass the listener to [`listen`].
    Primary(TcpListener),
    /// Another instance is already running and has been asked to show itself.
    Secondary,
}

pub fn acquire() -> Instance {
    acquire_on(PORT)
}

fn acquire_on(port: u16) -> Instance {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => Instance::Primary(listener),
        Err(_) => match TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(1)) {
            Ok(mut stream) => {
                let _ = stream.write_all(HELLO);
                Instance::Secondary
            }
            // Something else holds the port; run anyway rather than refuse to start.
            Err(_) => Instance::Primary(TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind a loopback port")),
        },
    }
}

/// Call `on_show` on a background thread each time another instance starts.
pub fn listen(listener: TcpListener, on_show: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let mut buf = [0u8; HELLO.len()];
            if stream.read_exact(&mut buf).is_ok() && buf == HELLO {
                on_show();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn second_instance_asks_the_first_to_show() {
        let port = 53_995;
        let Instance::Primary(listener) = acquire_on(port) else { panic!("first instance should be primary") };
        let (tx, rx) = mpsc::channel();
        listen(listener, move || tx.send(()).unwrap());

        assert!(matches!(acquire_on(port), Instance::Secondary));
        rx.recv_timeout(Duration::from_secs(2)).expect("primary was asked to show");
    }
}

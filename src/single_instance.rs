//! Only one AirDash runs at a time: a second launch asks the first to show its window and exits.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

/// Next to the OAuth redirect port (53682).
const PORT: u16 = 53681;
const HELLO: &[u8] = b"airdash:show\n";
/// The running instance's answer, so a launch never mistakes another program on the port for AirDash.
const ACK: &[u8] = b"airdash:ok\n";
const TIMEOUT: Duration = Duration::from_secs(2);

pub enum Instance {
    /// This is the only instance; pass the listener to [`listen`].
    Primary(TcpListener),
    /// Another AirDash is running and confirmed it will show its window.
    Secondary,
}

pub fn acquire() -> Instance {
    acquire_on(PORT)
}

fn acquire_on(port: u16) -> Instance {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => Instance::Primary(listener),
        Err(_) if ask_running_instance(port) => Instance::Secondary,
        // Something other than AirDash holds the port: start normally, just without the single-instance check.
        Err(_) => Instance::Primary(TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind a loopback port")),
    }
}

/// True only when an AirDash on `port` acknowledged the request to show its window.
fn ask_running_instance(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let mut reply = [0u8; ACK.len()];
    stream.write_all(HELLO).is_ok() && stream.read_exact(&mut reply).is_ok() && reply == ACK
}

/// Answer other launches on a background thread, calling `on_show` for each. Start this as soon as the
/// instance is acquired, so a launch made while this one is still starting up gets its answer.
pub fn listen(listener: TcpListener, on_show: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(TIMEOUT));
            let _ = stream.set_write_timeout(Some(TIMEOUT));
            let mut buf = [0u8; HELLO.len()];
            if stream.read_exact(&mut buf).is_ok() && buf == HELLO {
                let _ = stream.write_all(ACK);
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

    #[test]
    fn another_program_on_the_port_does_not_stop_airdash_starting() {
        let port = 53_996;
        let other = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap();
        // Accepts connections but never speaks AirDash's protocol.
        std::thread::spawn(move || {
            for stream in other.incoming() {
                drop(stream);
            }
        });

        assert!(matches!(acquire_on(port), Instance::Primary(_)));
    }
}

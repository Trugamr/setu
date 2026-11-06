//! Link: the receiver's side of the UDP stream from the headset's capture server.
//!
//! A link says HELLO twice a second and hands every FORMAT and AUDIO it gets back to a callback,
//! on its own thread. It never gives up on the server: when nothing arrives it reports
//! [`Event::Quiet`] and keeps saying HELLO, so a server that starts later, or restarts, is
//! picked up without the caller doing anything.

use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::protocol::{self, Audio, Format, Message};

/// Well inside the server's 3-second stale timeout, so one lost HELLO never stops the stream.
const HELLO_INTERVAL: Duration = Duration::from_millis(500);
/// Silence this long means the server stopped streaming or went away.
pub const QUIET_AFTER: Duration = Duration::from_secs(1);
/// How often the thread wakes when nothing arrives, which bounds how long a drop takes.
const POLL: Duration = Duration::from_millis(100);
/// A large socket buffer rides out scheduling hiccups of the link thread.
const RECEIVE_BUFFER_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// The stream's format. The server sends it in answer to every HELLO and before the first
    /// AUDIO.
    Format(Format),
    Audio(Audio<'a>),
    /// Nothing has arrived for [`QUIET_AFTER`]. Reported once per silence.
    Quiet,
}

/// A running link. Dropping it stops the thread.
pub struct Link {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Link {
    /// Starts talking to the capture server at `server`, introducing this receiver as `name`.
    pub fn start<F>(server: SocketAddr, name: &str, mut on_event: F) -> io::Result<Link>
    where
        F: FnMut(Event<'_>) + Send + 'static,
    {
        let socket = open_socket(server)?;
        let hello = protocol::encode_hello(name);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new().name("setu-link".into()).spawn({
            let stop = stop.clone();
            move || run(&socket, &hello, &stop, &mut on_event)
        })?;
        Ok(Link {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(socket: &UdpSocket, hello: &[u8], stop: &AtomicBool, on_event: &mut dyn FnMut(Event<'_>)) {
    let mut buf = vec![0; 64 * 1024];
    let mut next_hello = Instant::now();
    let mut last_heard = Instant::now();
    // Nothing has been heard yet, which the caller already knows.
    let mut quiet = true;
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_hello {
            // Fails while the server is unreachable; the next HELLO tries again.
            let _ = socket.send(hello);
            next_hello = Instant::now() + HELLO_INTERVAL;
        }
        match socket.recv(&mut buf) {
            Ok(len) => match protocol::decode(&buf[..len]) {
                Ok(Message::Format(format)) => {
                    last_heard = Instant::now();
                    quiet = false;
                    on_event(Event::Format(format));
                }
                Ok(Message::Audio(audio)) => {
                    last_heard = Instant::now();
                    quiet = false;
                    on_event(Event::Audio(audio));
                }
                // Messages meant for other parties, and datagrams that are not Setu.
                _ => {}
            },
            Err(e) if is_transient(&e) => {}
            // Unexpected; back off rather than spin.
            Err(_) => thread::sleep(POLL),
        }
        if !quiet && last_heard.elapsed() >= QUIET_AFTER {
            quiet = true;
            on_event(Event::Quiet);
        }
    }
}

/// A receive timeout, or the ICMP "port unreachable" from a HELLO sent while no server was
/// listening, which Windows reports on the next receive.
fn is_transient(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionRefused
    )
}

/// A UDP socket connected to `server`, so the OS drops datagrams from anyone else.
fn open_socket(server: SocketAddr) -> io::Result<UdpSocket> {
    let socket = Socket::new(
        Domain::for_address(server),
        Type::DGRAM,
        Some(Protocol::UDP),
    )?;
    // Best effort: the OS may cap the size.
    let _ = socket.set_recv_buffer_size(RECEIVE_BUFFER_BYTES);
    let local: SocketAddr = match server {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    socket.bind(&local.into())?;
    socket.connect(&server.into())?;
    let socket = UdpSocket::from(socket);
    socket.set_read_timeout(Some(POLL))?;
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::fixture;
    use std::sync::mpsc::{self, Receiver};

    const WAIT: Duration = Duration::from_secs(3);

    /// An owned copy of an [`Event`].
    #[derive(Debug, PartialEq)]
    enum Seen {
        Format(Format),
        Audio { frame_position: u64, pcm: Vec<u8> },
        Quiet,
    }

    fn start(server: &UdpSocket) -> (Link, Receiver<Seen>) {
        let (tx, rx) = mpsc::channel();
        let link = Link::start(server.local_addr().unwrap(), "test-pc", move |event| {
            let seen = match event {
                Event::Format(format) => Seen::Format(format),
                Event::Audio(audio) => Seen::Audio {
                    frame_position: audio.frame_position,
                    pcm: audio.pcm.to_vec(),
                },
                Event::Quiet => Seen::Quiet,
            };
            let _ = tx.send(seen);
        })
        .unwrap();
        (link, rx)
    }

    fn fake_server() -> UdpSocket {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(WAIT)).unwrap();
        socket
    }

    /// Waits for a HELLO and returns who sent it.
    fn expect_hello(server: &UdpSocket) -> SocketAddr {
        let mut buf = [0; 1500];
        let (len, from) = server.recv_from(&mut buf).expect("no HELLO");
        assert_eq!(
            protocol::decode(&buf[..len]),
            Ok(Message::Hello {
                name: "test-pc".into()
            })
        );
        from
    }

    fn format() -> Format {
        Format {
            sample_rate: 48000,
            channels: 2,
            bytes_per_sample: 2,
            block_frames: 96,
        }
    }

    #[test]
    fn says_hello_twice_a_second() {
        let server = fake_server();
        let (_link, _events) = start(&server);
        expect_hello(&server);
        let first = Instant::now();
        expect_hello(&server);
        let gap = first.elapsed();
        assert!(gap >= Duration::from_millis(400), "HELLOs {gap:?} apart");
    }

    #[test]
    fn hands_format_and_audio_to_the_callback() {
        let server = fake_server();
        let (_link, events) = start(&server);
        let receiver = expect_hello(&server);
        server.send_to(&fixture!("format"), receiver).unwrap();
        server.send_to(&fixture!("audio"), receiver).unwrap();
        assert_eq!(events.recv_timeout(WAIT), Ok(Seen::Format(format())));
        assert_eq!(
            events.recv_timeout(WAIT),
            Ok(Seen::Audio {
                frame_position: 1000,
                pcm: vec![0x01, 0x00, 0xff, 0xff, 0x00, 0x01, 0x00, 0xff],
            })
        );
    }

    #[test]
    fn ignores_datagrams_that_are_not_for_it() {
        let server = fake_server();
        let (_link, events) = start(&server);
        let receiver = expect_hello(&server);
        server.send_to(b"QAHI", receiver).unwrap();
        server.send_to(&fixture!("status"), receiver).unwrap();
        server.send_to(&fixture!("format"), receiver).unwrap();
        assert_eq!(events.recv_timeout(WAIT), Ok(Seen::Format(format())));
    }

    #[test]
    fn reports_quiet_once_when_the_server_goes_silent() {
        let server = fake_server();
        let (_link, events) = start(&server);
        let receiver = expect_hello(&server);
        server.send_to(&fixture!("format"), receiver).unwrap();
        assert_eq!(events.recv_timeout(WAIT), Ok(Seen::Format(format())));
        let sent = Instant::now();
        assert_eq!(events.recv_timeout(WAIT), Ok(Seen::Quiet));
        assert!(sent.elapsed() >= QUIET_AFTER - POLL);
        assert!(events.recv_timeout(QUIET_AFTER * 2).is_err());
    }

    #[test]
    fn finds_a_server_that_starts_later() {
        let port = fake_server().local_addr().unwrap().port();
        let (tx, events) = mpsc::channel();
        let _link = Link::start(([127, 0, 0, 1], port).into(), "test-pc", move |event| {
            if let Event::Format(format) = event {
                let _ = tx.send(format);
            }
        })
        .unwrap();
        // Let a few HELLOs bounce off the closed port first.
        thread::sleep(HELLO_INTERVAL * 3);
        let server = UdpSocket::bind(("127.0.0.1", port)).unwrap();
        server.set_read_timeout(Some(WAIT)).unwrap();
        let receiver = expect_hello(&server);
        server.send_to(&fixture!("format"), receiver).unwrap();
        assert_eq!(events.recv_timeout(WAIT), Ok(format()));
    }

    #[test]
    fn dropping_stops_the_link_promptly() {
        let server = fake_server();
        let (link, events) = start(&server);
        expect_hello(&server);
        let dropped = Instant::now();
        drop(link);
        assert!(dropped.elapsed() < POLL * 3);
        // The callback, and the channel sender with it, is gone.
        assert_eq!(events.recv(), Err(mpsc::RecvError));
    }
}

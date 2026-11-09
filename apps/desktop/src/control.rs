//! Control: lets another process drive a running [`Session`], such as `setu-cli status` asking
//! the window what it is playing.
//!
//! The host (the window, or `setu-cli play`) binds [`Server`] to [`CONTROL_PORT`] on 127.0.0.1
//! and calls [`Server::answer`] from the thread that owns its session. Clients use [`send`].
//!
//! One request per connection: the client writes one line and the host answers and closes.
//!
//! | Request | Does |
//! | --- | --- |
//! | `status` | nothing; the reply carries the snapshot |
//! | `connect [headset]` | [`Command::Connect`] |
//! | `disconnect` | [`Command::Disconnect`] |
//! | `device [name]` | [`Command::Device`]; no name follows the system default |
//! | `exclusive on\|off` | [`Command::Exclusive`] |
//!
//! The reply's first line is `ok` or `error: <reason>`. After `ok`, and after an error from the
//! session, come the session's [`Snapshot`] as `key=value` lines.
//!
//! Only the first line of a connection is read, so a web page posting to the port sends
//! `POST / HTTP/1.1`, which is not a request, and changes nothing.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::session::{Command, Running, Session, Snapshot};
use crate::view::Phase;

/// The control port on 127.0.0.1, one above the headset's UDP port.
pub const CONTROL_PORT: u16 = crate::protocol::UDP_PORT + 1;

/// How long the host may take to answer, which includes opening the output device.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// Longer request lines are cut here and fail to parse.
const MAX_LINE: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Status,
    Do(Command),
}

impl Request {
    pub fn parse(line: &str) -> Result<Request, String> {
        let line = line.trim();
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let arg = (!rest.is_empty()).then(|| rest.to_string());
        let command = match (verb, arg) {
            ("status", None) => return Ok(Request::Status),
            ("connect", headset) => Command::Connect(headset),
            ("disconnect", None) => Command::Disconnect,
            ("device", name) => Command::Device(name),
            ("exclusive", Some(on)) if on == "on" => Command::Exclusive(true),
            ("exclusive", Some(off)) if off == "off" => Command::Exclusive(false),
            _ => return Err(format!("not a request: {line:?}")),
        };
        Ok(Request::Do(command))
    }

    pub fn to_line(&self) -> String {
        let with = |verb: &str, arg: &Option<String>| match arg {
            Some(arg) => format!("{verb} {arg}"),
            None => verb.into(),
        };
        match self {
            Request::Status => "status".into(),
            Request::Do(Command::Connect(headset)) => with("connect", headset),
            Request::Do(Command::Disconnect) => "disconnect".into(),
            Request::Do(Command::Device(name)) => with("device", name),
            Request::Do(Command::Exclusive(on)) => {
                format!("exclusive {}", if *on { "on" } else { "off" })
            }
        }
    }
}

/// A host's reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Why the request failed, if it did.
    pub error: Option<String>,
    /// The session's snapshot as `key=value` pairs, in order.
    pub fields: Vec<(String, String)>,
}

impl Reply {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn parse(text: &str) -> io::Result<Reply> {
        let mut lines = text.lines();
        let error = match lines.next() {
            Some("ok") => None,
            Some(line) if line.starts_with("error: ") => Some(line["error: ".len()..].into()),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "not a Setu control reply",
                ));
            }
        };
        let fields = lines
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Ok(Reply { error, fields })
    }
}

/// Sends a request to the app running on this PC.
pub fn send(request: &Request) -> io::Result<Reply> {
    send_to(
        SocketAddr::from((Ipv4Addr::LOCALHOST, CONTROL_PORT)),
        request,
    )
}

fn send_to(addr: SocketAddr, request: &Request) -> io::Result<Reply> {
    let mut stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)?;
    stream.set_read_timeout(Some(ANSWER_TIMEOUT + IO_TIMEOUT))?;
    stream.write_all(format!("{}\n", request.to_line()).as_bytes())?;
    let mut text = String::new();
    stream.read_to_string(&mut text)?;
    Reply::parse(&text)
}

/// Accepts requests on a background thread and queues them for [`Server::answer`]. Dropping it
/// closes the port.
pub struct Server {
    requests: mpsc::Receiver<Pending>,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

struct Pending {
    request: Request,
    reply: mpsc::Sender<String>,
}

impl Server {
    /// Listens on [`CONTROL_PORT`]. Fails when another Setu already listens there.
    pub fn bind() -> io::Result<Server> {
        Server::bind_to(SocketAddr::from((Ipv4Addr::LOCALHOST, CONTROL_PORT)))
    }

    fn bind_to(addr: SocketAddr) -> io::Result<Server> {
        let listener = TcpListener::bind(addr)?;
        let addr = listener.local_addr()?;
        let (requests_tx, requests) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        thread::Builder::new().name("setu-control".into()).spawn({
            let stop = stop.clone();
            move || listen(listener, requests_tx, &stop)
        })?;
        Ok(Server {
            requests,
            addr,
            stop,
        })
    }

    /// Applies the requests that arrived since the last call and answers them. Hosts call it a
    /// few times a second; a client waits for it.
    pub fn answer<R: Running>(&self, session: &mut Session<R>) {
        while let Ok(pending) = self.requests.try_recv() {
            let result = match pending.request {
                Request::Status => Ok(()),
                Request::Do(command) => session.apply(command),
            };
            let _ = pending.reply.send(reply_text(result, &session.snapshot()));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wakes the listening thread so it sees `stop`.
        let _ = TcpStream::connect_timeout(&self.addr, IO_TIMEOUT);
    }
}

fn listen(listener: TcpListener, requests: mpsc::Sender<Pending>, stop: &AtomicBool) {
    for stream in listener.incoming() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Ok(stream) = stream {
            let _ = serve(stream, &requests);
        }
    }
}

fn serve(stream: TcpStream, requests: &mpsc::Sender<Pending>) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut line = String::new();
    BufReader::new((&stream).take(MAX_LINE)).read_line(&mut line)?;
    let reply = match Request::parse(&line) {
        Err(e) => format!("error: {e}\n"),
        Ok(request) => {
            let (reply, answer) = mpsc::channel();
            if requests.send(Pending { request, reply }).is_err() {
                return Ok(());
            }
            answer
                .recv_timeout(ANSWER_TIMEOUT)
                .unwrap_or_else(|_| "error: the app did not answer\n".into())
        }
    };
    (&stream).write_all(reply.as_bytes())
}

fn reply_text(result: Result<(), String>, snapshot: &Snapshot) -> String {
    let mut text = match result {
        Ok(()) => "ok\n".to_string(),
        Err(e) => format!("error: {}\n", one_line(&e)),
    };
    let s = &snapshot.settings;
    let fields = [
        ("phase", phase_name(snapshot.summary.phase).to_string()),
        ("title", snapshot.summary.title.clone()),
        ("detail", snapshot.summary.detail.clone()),
        ("connected", snapshot.connected.to_string()),
        ("headset", s.headset.clone()),
        ("device", s.device.clone().unwrap_or_default()),
        ("exclusive", s.exclusive.to_string()),
        ("error", snapshot.error.clone().unwrap_or_default()),
        ("level", format!("{:.2}", snapshot.summary.level)),
        ("stats", snapshot.summary.stats.clone().unwrap_or_default()),
    ];
    for (key, value) in fields {
        text.push_str(&format!("{key}={}\n", one_line(&value)));
    }
    text
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Stopped => "stopped",
        Phase::Failed => "failed",
        Phase::Waiting => "waiting",
        Phase::Buffering => "buffering",
        Phase::Playing => "playing",
        Phase::Unsupported => "unsupported",
        Phase::OutputLost => "output-lost",
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Status;
    use crate::settings::Settings;

    /// Never started: the sessions in these tests fail every connect.
    struct NoReceiver;

    impl Running for NoReceiver {
        fn status(&self) -> Status {
            unreachable!("no receiver starts in these tests")
        }
    }

    fn session(headset: &str) -> Session<NoReceiver> {
        let settings = Settings {
            headset: headset.into(),
            ..Settings::default()
        };
        Session::with_start(settings, None, |_| Err("no output in tests".into()))
    }

    /// Sends `request` to a server on a free port while `session` answers, and returns the reply.
    fn exchange(session: &mut Session<NoReceiver>, request: &Request) -> Reply {
        let server = Server::bind_to(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let addr = server.addr;
        let request = request.clone();
        let client = thread::spawn(move || send_to(addr, &request));
        while !client.is_finished() {
            server.answer(session);
            thread::sleep(Duration::from_millis(5));
        }
        client.join().unwrap().unwrap()
    }

    #[test]
    fn requests_round_trip_as_lines() {
        let requests = [
            Request::Status,
            Request::Do(Command::Connect(None)),
            Request::Do(Command::Connect(Some("192.168.1.42:27184".into()))),
            Request::Do(Command::Disconnect),
            Request::Do(Command::Device(None)),
            Request::Do(Command::Device(Some("Speakers (Realtek Audio)".into()))),
            Request::Do(Command::Exclusive(true)),
            Request::Do(Command::Exclusive(false)),
        ];
        for request in requests {
            assert_eq!(Request::parse(&request.to_line()), Ok(request));
        }
    }

    #[test]
    fn http_is_not_a_request() {
        assert!(Request::parse("POST / HTTP/1.1\r\n").is_err());
        assert!(Request::parse("exclusive maybe").is_err());
        assert!(Request::parse("status please").is_err());
    }

    #[test]
    fn status_reports_the_snapshot_and_changes_nothing() {
        let mut session = session("quest.local");
        let before = session.snapshot();
        let reply = exchange(&mut session, &Request::Status);
        assert_eq!(reply.error, None);
        assert_eq!(session.snapshot(), before);
        assert_eq!(reply.get("phase"), Some("stopped"));
        assert_eq!(reply.get("connected"), Some("false"));
        assert_eq!(reply.get("headset"), Some("quest.local"));
    }

    #[test]
    fn commands_change_the_hosts_session() {
        let mut session = session("");
        let reply = exchange(
            &mut session,
            &Request::Do(Command::Device(Some("Speakers".into()))),
        );
        assert_eq!(reply.error, None);
        assert_eq!(reply.get("device"), Some("Speakers"));
        assert_eq!(
            session.snapshot().settings.device.as_deref(),
            Some("Speakers")
        );
    }

    #[test]
    fn a_failed_command_replies_with_the_reason_and_the_snapshot() {
        let mut session = session("127.0.0.1");
        let reply = exchange(&mut session, &Request::Do(Command::Connect(None)));
        assert_eq!(reply.error.as_deref(), Some("no output in tests"));
        assert_eq!(reply.get("error"), Some("no output in tests"));
    }

    #[test]
    fn a_bad_line_gets_an_error_without_reaching_the_session() {
        let server = Server::bind_to(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let mut stream = TcpStream::connect(server.addr).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        assert!(text.starts_with("error: not a request"), "{text}");
        // Nothing was queued for the session.
        assert!(server.requests.try_recv().is_err());
    }
}

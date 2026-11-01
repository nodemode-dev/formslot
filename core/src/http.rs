use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{Result, log, redact};

const HEADERS_MAX: usize = 64 * 1024;
const BODY_MAX: usize = 1024 * 1024;
const MAX_CONNECTIONS: usize = 64;
const READ_DEADLINE: Duration = Duration::from_secs(15);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

static OPEN_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

struct Slot;

impl Slot {
    fn take() -> Option<Slot> {
        if OPEN_CONNECTIONS.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            OPEN_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        OPEN_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Vec<u8>,
}

pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        421 => "Misdirected Request",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

impl Reply {
    pub fn json(status: u16, value: &Value) -> Reply {
        Reply { status, body: value.to_string().into_bytes(), headers: Vec::new() }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Reply {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn ok(value: &Value) -> Reply {
        Reply::json(200, value)
    }

    pub fn error(status: u16, code: &str, message: &str) -> Reply {
        Reply::json(status, &json!({ "error": code, "message": message }))
    }
}

pub struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Server {
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    pub fn wait(mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.halt();
    }
}

struct Head {
    request: Request,
    headers: HashMap<String, String>,
}

fn bad() -> Reply {
    Reply::error(400, "bad_request", "The request could not be read.")
}

fn timed_out() -> Reply {
    Reply::error(408, "request_timeout", "The request was not received in time.")
}

fn arm(reader: &BufReader<&TcpStream>, deadline: Instant) -> std::result::Result<(), Reply> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(timed_out());
    }
    reader.get_ref().set_read_timeout(Some(left)).map_err(|_| bad())
}

fn io_reply(e: &std::io::Error) -> Reply {
    match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => timed_out(),
        _ => bad(),
    }
}

fn read_line(reader: &mut BufReader<&TcpStream>, used: &mut usize, deadline: Instant) -> std::result::Result<String, Reply> {
    let mut raw = Vec::new();
    let room = (HEADERS_MAX.saturating_sub(*used) + 1) as u64;
    arm(reader, deadline)?;
    let n = reader.by_ref().take(room).read_until(b'\n', &mut raw).map_err(|e| io_reply(&e))?;
    *used += n;
    if n == 0 || *used > HEADERS_MAX || !raw.ends_with(b"\n") {
        return Err(bad());
    }
    let line = String::from_utf8(raw).map_err(|_| bad())?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn read_request(stream: &TcpStream) -> std::result::Result<Head, Reply> {
    let mut reader = BufReader::new(stream);
    let mut used = 0;
    let deadline = Instant::now() + READ_DEADLINE;
    let first = read_line(&mut reader, &mut used, deadline)?;
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let (path, query) = target
        .split_once('?')
        .map(|(p, q)| (p.to_string(), q.to_string()))
        .unwrap_or((target, String::new()));
    let mut headers = HashMap::new();
    loop {
        let line = read_line(&mut reader, &mut used, deadline)?;
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let guarded = matches!(key.as_str(), "host" | "origin" | "authorization" | "content-length" | "transfer-encoding");
            if headers.insert(key, v.trim().to_string()).is_some() && guarded {
                return Err(Reply::error(400, "bad_request", "A header that is checked appears more than once."));
            }
        }
    }
    if !path.starts_with('/') {
        return Err(bad());
    }
    if headers.contains_key("transfer-encoding") {
        return Err(Reply::error(400, "bad_request", "Chunked bodies are not accepted; send a Content-Length."));
    }
    let mut body = Vec::new();
    if let Some(length) = headers.get("content-length") {
        let length: usize = length.parse().map_err(|_| bad())?;
        if length > BODY_MAX {
            return Err(Reply::error(413, "too_large", "Bodies are limited to 1 MiB."));
        }
        body = vec![0; length];
        let mut filled = 0;
        while filled < length {
            arm(&reader, deadline)?;
            match reader.read(&mut body[filled..]) {
                Ok(0) => return Err(bad()),
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(io_reply(&e)),
            }
        }
    }
    Ok(Head { request: Request { method, path, query, body }, headers })
}

fn write_reply(mut stream: &TcpStream, reply: &Reply) {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n{}",
        reply.status,
        reason(reply.status),
        reply.body.len(),
        reply.headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect::<String>()
    );
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(b"\r\n"))
        .and_then(|()| stream.write_all(&reply.body))
        .and_then(|()| stream.flush());
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn host_ok(value: &str, port: u16) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value == format!("127.0.0.1:{port}") || value == format!("localhost:{port}")
}

fn origin_ok(value: &str, port: u16) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value == format!("http://127.0.0.1:{port}") || value == format!("http://localhost:{port}")
}

fn refuse(reason: &str, status: u16, code: &str, message: &str) -> Reply {
    log::record("http-refused", json!({ "reason": reason, "status": status }));
    Reply::error(status, code, message)
}

fn handle(head: Head, port: u16, token: &str, route: fn(&Request) -> Reply) -> Reply {
    let Head { request, headers } = head;
    if !headers.get("host").is_some_and(|host| host_ok(host, port)) {
        return refuse("host", 421, "wrong_host", "The Host header must be 127.0.0.1 or localhost with this service's port.");
    }
    if headers.get("origin").is_some_and(|origin| !origin_ok(origin, port)) {
        return refuse("origin", 403, "wrong_origin", "The Origin header is not this computer.");
    }
    let given = headers
        .get("authorization")
        .and_then(|a| a.strip_prefix("Bearer ").or_else(|| a.strip_prefix("bearer ")))
        .unwrap_or("")
        .trim();
    if !same(given, token) {
        return refuse("token", 401, "unauthorized", "A valid access token is required.");
    }
    route(&request)
}

pub fn start(port: u16, token: String, route: fn(&Request) -> Reply) -> Result<Server> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
        .map_err(|e| format!("127.0.0.1:{port} could not be opened ({e}); another program may use it, or set FORMSLOT_PORT"))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?.port();
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    redact::remember(&token);
    let token = Arc::new(token);
    let stop = Arc::new(AtomicBool::new(false));
    let halt = Arc::clone(&stop);
    log::record("http-start", json!({ "port": bound }));
    let handle = thread::spawn(move || {
        while !halt.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                    let Some(slot) = Slot::take() else {
                        log::record("http-refused", json!({ "reason": "busy", "status": 503 }));
                        write_reply(&stream, &Reply::error(503, "busy", "Too many connections are open; try again shortly.").with_header("Retry-After", "1"));
                        continue;
                    };
                    let token = Arc::clone(&token);
                    thread::spawn(move || {
                        let _slot = slot;
                        let reply = match read_request(&stream) {
                            Ok(head) => handle(head, bound, &token, route),
                            Err(reply) => reply,
                        };
                        write_reply(&stream, &reply);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
                Err(_) => thread::sleep(Duration::from_millis(200)),
            }
        }
        log::record("http-stop", json!({ "port": bound }));
    });
    Ok(Server { port: bound, stop, handle: Some(handle) })
}

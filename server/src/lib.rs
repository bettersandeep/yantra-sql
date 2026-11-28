//! lekha-server: a small HTTP server for the lekha SQL engine.
//!
//! Each request is hostile input.
//! The code does not index into request data and does not unwrap it.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::catch_unwind;
use std::sync::mpsc::{TrySendError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use lekha::Value;

/// The request line and the headers, with the empty line that ends them.
pub const MAX_HEAD: usize = 8 * 1024;
/// The SQL text.
pub const MAX_BODY: usize = 2000;
const WORKERS: usize = 4;
const QUEUE: usize = 16;

const INDEX: &str = "lekha-server: the lekha SQL engine behind HTTP.
Send one SQL statement as the body of POST /query. The answer is JSON.
curl --data-binary 'SELECT 7 / 2' https://<this host>/query
";
const PREFLIGHT: &str = "Access-Control-Allow-Methods: POST, OPTIONS\r\n\
Access-Control-Allow-Headers: content-type\r\n\
Access-Control-Max-Age: 600\r\n";

pub struct Config {
    /// The value of `Access-Control-Allow-Origin`. Empty: no CORS header.
    pub allow_origin: String,
    /// The time limit to read one request, and the limit of each write.
    pub timeout: Duration,
    /// Print one line per request to stdout.
    pub log: bool,
}

#[derive(Debug, PartialEq)]
pub struct Request {
    pub method: String,
    /// The path without the query string.
    pub path: String,
    pub body: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    /// More header lines. Each one has its line end.
    pub extra: String,
    pub body: String,
}

impl Response {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "application/json",
            extra: String::new(),
            body,
        }
    }

    fn error(status: u16) -> Self {
        Self::json(status, error_json(&reason(status).to_lowercase()))
    }

    /// The bytes of the response. `allow_origin` is empty when CORS is off.
    pub fn to_http(&self, allow_origin: &str) -> String {
        let mut out = format!("HTTP/1.1 {} {}\r\n", self.status, reason(self.status));
        // RFC 9110 section 8.6: a 204 response does not have Content-Length.
        if self.status != 204 {
            let _ = write!(
                out,
                "Content-Type: {}\r\nContent-Length: {}\r\n",
                self.content_type,
                self.body.len()
            );
        }
        out.push_str(
            "Connection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n",
        );
        if !allow_origin.is_empty() {
            let _ = write!(out, "Access-Control-Allow-Origin: {allow_origin}\r\n");
        }
        out.push_str(&self.extra);
        out.push_str("\r\n");
        out.push_str(&self.body);
        out
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    }
}

/// Appends `text` as a JSON string.
pub fn json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn error_json(message: &str) -> String {
    let mut out = String::from(r#"{"ok":false,"error":"#);
    json_string(&mut out, message);
    out.push('}');
    out
}

type Rows = Vec<Vec<Value>>;

/// Runs one statement and returns the JSON body of the answer.
pub fn query_json(sql: &str) -> String {
    // The engine limits the nesting and the size of an expression, so a
    // statement of MAX_BODY bytes cannot run long. There is no time limit.
    result_json(catch_unwind(|| lekha::Database::new().execute(sql)))
}

/// The JSON body for a result of the engine. The outer error is a panic.
pub fn result_json(result: thread::Result<Result<Rows, lekha::Error>>) -> String {
    let rows = match result {
        Ok(Ok(rows)) => rows,
        Ok(Err(error)) => return error_json(&error.to_string()),
        Err(_) => return error_json("the engine stopped on this input"),
    };
    let mut out = String::from(r#"{"ok":true,"rows":["#);
    for (r, row) in rows.iter().enumerate() {
        if r > 0 {
            out.push(',');
        }
        out.push('[');
        for (c, value) in row.iter().enumerate() {
            if c > 0 {
                out.push(',');
            }
            match value {
                Value::Null => out.push_str("null"),
                Value::Integer(n) => {
                    let _ = write!(out, "\"{n}\"");
                }
                Value::Text(text) => json_string(&mut out, text),
                // The engine returns no real value today. Do not guess a format.
                Value::Real(_) => {
                    return error_json("this server cannot print a real number yet");
                }
            }
        }
        out.push(']');
    }
    out.push_str("]}");
    out
}

/// Parses the request line and the headers.
/// Returns the method, the path without the query string, and the length of the body.
/// The error is the HTTP status of the answer.
pub fn parse_head(head: &[u8]) -> Result<(String, String, usize), u16> {
    let head = std::str::from_utf8(head).map_err(|_| 400u16)?;
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(400);
    };
    // The access log prints the method and the path, so they are plain ASCII.
    let method_ok =
        !method.is_empty() && method.len() <= 16 && method.bytes().all(|b| b.is_ascii_uppercase());
    let target_ok = target.starts_with('/') && target.bytes().all(|b| b.is_ascii_graphic());
    if !method_ok || !target_ok || !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(400);
    }
    let path = target.split('?').next().unwrap_or(target);
    let mut length = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(400u16)?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(411);
        }
        if name.eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            let digits = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
            if length.is_some() || !digits {
                return Err(400);
            }
            // Digits that do not parse are a number that is too large.
            let n: usize = value.parse().map_err(|_| 413u16)?;
            if n > MAX_BODY {
                return Err(413);
            }
            length = Some(n);
        }
    }
    match length {
        Some(n) => Ok((method.to_string(), path.to_string(), n)),
        None if method == "POST" => Err(411),
        None => Ok((method.to_string(), path.to_string(), 0)),
    }
}

/// Appends the next bytes of the stream to `buf`.
/// The end of the stream is a malformed request. An error is a timeout.
fn read_some(reader: &mut impl Read, buf: &mut Vec<u8>) -> Result<(), u16> {
    let mut chunk = [0u8; 1024];
    match reader.read(&mut chunk) {
        Ok(0) => Err(400),
        Ok(n) => {
            buf.extend_from_slice(chunk.get(..n).unwrap_or(&chunk));
            Ok(())
        }
        Err(_) => Err(408),
    }
}

/// Reads one request. The error is the HTTP status of the answer.
pub fn read_request(reader: &mut impl Read) -> Result<Request, u16> {
    let mut buf = Vec::new();
    let head_end = loop {
        let seen = buf.get(..MAX_HEAD).unwrap_or(&buf);
        if let Some(at) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if buf.len() >= MAX_HEAD {
            return Err(431);
        }
        read_some(reader, &mut buf)?;
    };
    let (method, path, length) = parse_head(buf.get(..head_end).ok_or(400u16)?)?;
    let mut body = buf.get(head_end + 4..).ok_or(400u16)?.to_vec();
    while body.len() < length {
        read_some(reader, &mut body)?;
    }
    body.truncate(length);
    Ok(Request { method, path, body })
}

/// The answer to one request. `allow_origin` is empty when CORS is off.
pub fn respond(request: &Request, allow_origin: &str) -> Response {
    let allow = match request.path.as_str() {
        "/" | "/health" => "GET",
        "/query" => "POST, OPTIONS",
        _ => return Response::error(404),
    };
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => Response {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            extra: String::new(),
            body: INDEX.to_string(),
        },
        ("GET", "/health") => Response::json(
            200,
            format!(
                r#"{{"ok":true,"engine":"lekha","version":"{}"}}"#,
                lekha::VERSION
            ),
        ),
        ("POST", "/query") => match std::str::from_utf8(&request.body) {
            Ok(sql) => Response::json(200, query_json(sql)),
            Err(_) => Response::error(400),
        },
        ("OPTIONS", "/query") => {
            let mut response = Response::json(204, String::new());
            if !allow_origin.is_empty() {
                response.extra = PREFLIGHT.to_string();
            }
            response
        }
        _ => {
            let mut response = Response::error(405);
            response.extra = format!("Allow: {allow}\r\n");
            response
        }
    }
}

/// Gives the read of one request one deadline.
/// A timeout for each read is not sufficient: a client that sends one byte
/// every few seconds would hold a worker for hours.
struct Deadline<'a> {
    stream: &'a TcpStream,
    end: Instant,
}

impl Read for Deadline<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

/// Writes the response and the access log line. The log never has the SQL text.
fn finish(
    mut stream: &TcpStream,
    config: &Config,
    start: Instant,
    (method, path, bytes_in): (&str, &str, usize),
    response: &Response,
) {
    let _ = stream.set_write_timeout(Some(config.timeout));
    let _ = stream.write_all(response.to_http(&config.allow_origin).as_bytes());
    if config.log {
        // println! panics when stdout is closed. A worker must not stop for that.
        let _ = writeln!(
            io::stdout(),
            "{method} {} {} in={bytes_in} ms={}",
            path.get(..64).unwrap_or(path),
            response.status,
            start.elapsed().as_millis()
        );
    }
}

fn handle(stream: &TcpStream, config: &Config) {
    let start = Instant::now();
    let mut reader = Deadline {
        stream,
        end: start + config.timeout,
    };
    match read_request(&mut reader) {
        Ok(request) => finish(
            stream,
            config,
            start,
            (&request.method, &request.path, request.body.len()),
            &respond(&request, &config.allow_origin),
        ),
        Err(status) => finish(
            stream,
            config,
            start,
            ("-", "-", 0),
            &Response::error(status),
        ),
    }
}

/// Serves connections until the process stops.
/// A fixed pool of workers takes connections from a bounded queue.
pub fn serve(listener: TcpListener, config: Config) {
    let config = Arc::new(config);
    let (queue, jobs) = sync_channel::<TcpStream>(QUEUE);
    let jobs = Arc::new(Mutex::new(jobs));
    for _ in 0..WORKERS {
        let (jobs, config) = (Arc::clone(&jobs), Arc::clone(&config));
        thread::spawn(move || {
            loop {
                // This statement ends before the work starts, so the lock is free again.
                let job = jobs.lock().unwrap_or_else(PoisonError::into_inner).recv();
                match job {
                    Ok(stream) => handle(&stream, &config),
                    Err(_) => return,
                }
            }
        });
    }
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            // Do not spin when accept fails again and again.
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        if let Err(TrySendError::Full(stream)) = queue.try_send(stream) {
            // The answer is small, so this write does not block the accept loop.
            let response = Response::error(503);
            finish(&stream, &config, Instant::now(), ("-", "-", 0), &response);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bytes: &[u8]) -> Result<Request, u16> {
        read_request(&mut &bytes[..])
    }

    fn request(method: &str, path: &str, body: &[u8]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            body: body.to_vec(),
        }
    }

    /// Gives one byte per read, as a slow client does.
    struct OneByte<'a>(&'a [u8]);

    impl Read for OneByte<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.0.len().min(1);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn good_requests() {
        assert_eq!(
            read(b"GET /health HTTP/1.1\r\nHost: a\r\n\r\n"),
            Ok(request("GET", "/health", b""))
        );
        let post = b"POST /query HTTP/1.1\r\nHost: a\r\ncontent-LENGTH:  8 \r\n\r\nSELECT 1";
        assert_eq!(read(post), Ok(request("POST", "/query", b"SELECT 1")));
        assert_eq!(
            read_request(&mut OneByte(post)),
            Ok(request("POST", "/query", b"SELECT 1"))
        );
        // Bytes after the body are not part of the request.
        assert_eq!(
            read(b"POST /query HTTP/1.0\r\nContent-Length: 2\r\n\r\nabcd"),
            Ok(request("POST", "/query", b"ab"))
        );
    }

    #[test]
    fn post_without_content_length() {
        assert_eq!(read(b"POST /query HTTP/1.1\r\nHost: a\r\n\r\n"), Err(411));
        let chunked = b"POST /query HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
        assert_eq!(read(chunked), Err(411));
    }

    #[test]
    fn body_limit() {
        let post = |n: usize| {
            let mut bytes = format!("POST /query HTTP/1.1\r\nContent-Length: {n}\r\n\r\n");
            bytes.push_str(&" ".repeat(n));
            read(bytes.as_bytes())
        };
        assert_eq!(post(MAX_BODY).map(|r| r.body.len()), Ok(MAX_BODY));
        assert_eq!(post(MAX_BODY + 1), Err(413));
        let huge = b"POST /query HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n";
        assert_eq!(read(huge), Err(413));
    }

    #[test]
    fn head_limit() {
        let get = |head_len: usize| {
            let start = "GET / HTTP/1.1\r\nX: ";
            let fill = "a".repeat(head_len - start.len() - 4);
            read(format!("{start}{fill}\r\n\r\n").as_bytes())
        };
        assert_eq!(get(MAX_HEAD), Ok(request("GET", "/", b"")));
        assert_eq!(get(MAX_HEAD + 1), Err(431));
        assert_eq!(read(&[b'a'; 20_000]), Err(431));
    }

    #[test]
    fn unknown_method() {
        let brew = read(b"BREW /query HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(brew, request("BREW", "/query", b""));
        let response = respond(&brew, "");
        assert_eq!(response.status, 405);
        assert_eq!(response.extra, "Allow: POST, OPTIONS\r\n");
        assert_eq!(respond(&request("POST", "/health", b""), "").status, 405);
        assert_eq!(respond(&request("BREW", "/nothing", b""), "").status, 404);
    }

    #[test]
    fn path_with_query_string() {
        let health = read(b"GET /health?a=1&b=/query HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(health, request("GET", "/health", b""));
        assert_eq!(respond(&health, "").status, 200);
    }

    #[test]
    fn invalid_utf8() {
        let post = read(b"POST /query HTTP/1.1\r\nContent-Length: 2\r\n\r\n\xff\xfe").unwrap();
        assert_eq!(respond(&post, "").status, 400);
        assert_eq!(read(b"GET / HTTP/1.1\r\nX: \xff\r\n\r\n"), Err(400));
    }

    #[test]
    fn malformed_requests() {
        for bytes in [
            &b""[..],
            b"\r\n\r\n",
            b"GET /\r\n\r\n",
            b"GET / HTTP/1.1 x\r\n\r\n",
            b"GET / HTTP/2\r\n\r\n",
            b"get / HTTP/1.1\r\n\r\n",
            b"GET health HTTP/1.1\r\n\r\n",
            b"GET /\x1b[31m HTTP/1.1\r\n\r\n",
            b"GET / HTTP/1.1\r\nno colon\r\n\r\n",
            b"GET / HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
            b"GET / HTTP/1.1\r\nContent-Length: +1\r\n\r\na",
            b"GET / HTTP/1.1\r\nContent-Length:\r\n\r\n",
            b"POST /query HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\na",
            b"POST /query HTTP/1.1\r\nContent-Length: 5\r\n\r\nabc",
            b"GET / HTTP/1.1\r\nHost: a\r\n",
        ] {
            assert_eq!(read(bytes), Err(400), "{}", bytes.escape_ascii());
        }
    }

    #[test]
    fn json_escapes() {
        let mut out = String::new();
        json_string(&mut out, "a\"b\\c\nd\u{1}e\u{7f}\u{e9}");
        assert_eq!(out, "\"a\\\"b\\\\c\\u000ad\\u0001e\u{7f}\u{e9}\"");
    }

    #[test]
    fn query_results() {
        assert_eq!(
            query_json("SELECT 7 / 2, 1 / 0"),
            r#"{"ok":true,"rows":[["3",null]]}"#
        );
        assert_eq!(
            query_json("SELECT 1 FROM t"),
            r#"{"ok":false,"error":"unsupported syntax at Word(\"FROM\")"}"#
        );
        let rows = vec![
            vec![Value::Text("a\"b".to_string()), Value::Null],
            vec![Value::Integer(i64::MIN)],
        ];
        assert_eq!(
            result_json(Ok(Ok(rows))),
            r#"{"ok":true,"rows":[["a\"b",null],["-9223372036854775808"]]}"#
        );
        assert_eq!(
            result_json(Ok(Ok(vec![vec![Value::Integer(1), Value::Real(1.5)]]))),
            r#"{"ok":false,"error":"this server cannot print a real number yet"}"#
        );
    }

    #[test]
    fn engine_panic_is_an_error_answer() {
        assert_eq!(
            result_json(catch_unwind(|| panic!("a test panic"))),
            r#"{"ok":false,"error":"the engine stopped on this input"}"#
        );
    }

    #[test]
    fn response_headers() {
        let ok = respond(&request("GET", "/health", b""), "").to_http("");
        let body = format!(
            r#"{{"ok":true,"engine":"lekha","version":"{}"}}"#,
            lekha::VERSION
        );
        assert_eq!(
            ok,
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n\r\n{body}",
                body.len()
            )
        );
        let preflight = respond(&request("OPTIONS", "/query", b""), "https://a.example")
            .to_http("https://a.example");
        assert_eq!(
            preflight,
            "HTTP/1.1 204 No Content\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: https://a.example\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Max-Age: 600\r\n\r\n"
        );
        let index = respond(&request("GET", "/", b""), "");
        assert_eq!(index.content_type, "text/plain; charset=utf-8");
        assert_eq!(index.body.lines().count(), 3);
    }
}

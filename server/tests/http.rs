//! Starts the server inside the test process and talks to it over loopback.
//! `cargo test -- --nocapture` prints each request and its raw response.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use lekha_server::{Config, serve};

const LONG: Duration = Duration::from_secs(5);
const SHORT: Duration = Duration::from_millis(300);
const HEALTH: &[u8] = b"GET /health HTTP/1.1\r\nHost: test\r\n\r\n";

fn start(allow_origin: &str, timeout: Duration) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config {
        allow_origin: allow_origin.to_string(),
        timeout,
        log: false,
    };
    thread::spawn(move || serve(listener, config));
    addr
}

/// Sends the bytes and returns all that the server sends before it closes.
/// The read timeout makes a server that hangs fail the test.
fn send(addr: SocketAddr, request: &[u8]) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    // The server can close before it reads all of a request that it refuses.
    // Then the write or the read can fail, but the answer is in `answer`.
    let _ = stream.write_all(request);
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer).into_owned();
    println!("> {}\n< {answer}\n", request.escape_ascii());
    answer
}

fn post(addr: SocketAddr, sql: &str) -> String {
    let head = format!(
        "POST /query HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\n\r\n",
        sql.len()
    );
    send(addr, format!("{head}{sql}").as_bytes())
}

fn status(answer: &str) -> &str {
    answer.get(..12).unwrap_or(answer)
}

fn body(answer: &str) -> &str {
    answer.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

#[test]
fn health() {
    let answer = send(start("", LONG), HEALTH);
    assert_eq!(status(&answer), "HTTP/1.1 200");
    for header in [
        "Content-Type: application/json\r\n",
        "Connection: close\r\n",
        "X-Content-Type-Options: nosniff\r\n",
        "Cache-Control: no-store\r\n",
    ] {
        assert!(answer.contains(header), "{answer}");
    }
    let expected = format!(
        r#"{{"ok":true,"engine":"lekha","version":"{}"}}"#,
        lekha::VERSION
    );
    assert_eq!(body(&answer), expected);
    assert!(answer.contains(&format!("Content-Length: {}\r\n", expected.len())));
}

#[test]
fn query() {
    let addr = start("", LONG);
    let answer = post(addr, "SELECT 7 / 2, -7 / 2, 7 % -2");
    assert_eq!(status(&answer), "HTTP/1.1 200");
    assert_eq!(body(&answer), r#"{"ok":true,"rows":[["3","-3","1"]]}"#);
    assert_eq!(
        body(&post(addr, "SELECT 1 / 0")),
        r#"{"ok":true,"rows":[[null]]}"#
    );
    let answer = post(addr, "SELECT 1 FROM t");
    assert_eq!(status(&answer), "HTTP/1.1 200");
    assert!(
        body(&answer).starts_with(r#"{"ok":false,"error":""#),
        "{answer}"
    );
}

#[test]
fn index_not_found_and_wrong_method() {
    let addr = start("", LONG);
    let answer = send(addr, b"GET / HTTP/1.1\r\nHost: test\r\n\r\n");
    assert_eq!(status(&answer), "HTTP/1.1 200");
    assert!(answer.contains("Content-Type: text/plain"), "{answer}");
    let answer = send(addr, b"GET /nothing HTTP/1.1\r\nHost: test\r\n\r\n");
    assert_eq!(status(&answer), "HTTP/1.1 404");
    let answer = send(addr, b"GET /query HTTP/1.1\r\nHost: test\r\n\r\n");
    assert_eq!(status(&answer), "HTTP/1.1 405");
    assert!(answer.contains("Allow: POST, OPTIONS\r\n"), "{answer}");
}

#[test]
fn oversized_body_is_413() {
    let addr = start("", LONG);
    assert_eq!(status(&post(addr, &" ".repeat(2001))), "HTTP/1.1 413");
    assert_eq!(status(&send(addr, HEALTH)), "HTTP/1.1 200");
}

#[test]
fn garbage_is_400_and_the_server_still_answers() {
    let addr = start("", LONG);
    let answer = send(addr, b"\x00\xff\x16\x03\x01 garbage \r\n\r\n");
    assert_eq!(status(&answer), "HTTP/1.1 400");
    assert_eq!(status(&send(addr, HEALTH)), "HTTP/1.1 200");
}

#[test]
fn silent_client_is_dropped_by_the_timeout() {
    let addr = start("", SHORT);
    let begin = Instant::now();
    let answer = send(addr, b"");
    assert_eq!(status(&answer), "HTTP/1.1 408");
    assert!(begin.elapsed() >= SHORT - Duration::from_millis(50));
    assert_eq!(status(&send(addr, HEALTH)), "HTTP/1.1 200");
}

#[test]
fn slow_client_is_cut_at_the_deadline() {
    let addr = start("", SHORT);
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let begin = Instant::now();
    let mut answer = [0u8; 12];
    // One byte each 100 ms. Each read of the server gets a byte in time,
    // so only a deadline for the whole request stops this client.
    loop {
        let _ = stream.write_all(b"G");
        match stream.read(&mut answer) {
            Ok(0) => panic!("the server closed without an answer"),
            Ok(_) => break,
            Err(_) => assert!(begin.elapsed() < LONG, "the server did not cut the client"),
        }
    }
    assert_eq!(&answer, b"HTTP/1.1 408");
    assert_eq!(status(&send(addr, HEALTH)), "HTTP/1.1 200");
}

#[test]
fn cors_headers_only_when_configured() {
    let preflight = b"OPTIONS /query HTTP/1.1\r\nHost: test\r\nOrigin: https://a.example\r\n\r\n";
    let addr = start("https://a.example", LONG);
    let answer = send(addr, preflight);
    assert_eq!(status(&answer), "HTTP/1.1 204");
    for header in [
        "Access-Control-Allow-Origin: https://a.example\r\n",
        "Access-Control-Allow-Methods: POST, OPTIONS\r\n",
        "Access-Control-Allow-Headers: content-type\r\n",
        "Access-Control-Max-Age: 600\r\n",
    ] {
        assert!(answer.contains(header), "{answer}");
    }
    let allow = "Access-Control-Allow-Origin: https://a.example\r\n";
    assert!(post(addr, "SELECT 1").contains(allow));
    assert!(send(addr, b"garbage\r\n\r\n").contains(allow));

    let addr = start("", LONG);
    let answer = send(addr, preflight);
    assert_eq!(status(&answer), "HTTP/1.1 204");
    assert!(!answer.contains("Access-Control"), "{answer}");
    assert!(!post(addr, "SELECT 1").contains("Access-Control"));
}

#[test]
fn thirty_parallel_requests_all_get_an_answer() {
    let addr = start("", LONG);
    let clients: Vec<_> = (0..30)
        .map(|_| thread::spawn(move || post(addr, "SELECT 1")))
        .collect();
    for client in clients {
        let answer = client.join().unwrap();
        let status = status(&answer);
        assert!(
            status == "HTTP/1.1 200" || status == "HTTP/1.1 503",
            "{answer}"
        );
    }
}

#[test]
fn full_queue_is_503() {
    let addr = start("", LONG);
    // Silent clients hold the 4 workers. The pause lets each worker take one.
    // More silent clients then fill the 16 places in the queue.
    let mut silent: Vec<TcpStream> = (0..4).map(|_| TcpStream::connect(addr).unwrap()).collect();
    thread::sleep(Duration::from_millis(300));
    silent.extend((0..20).map(|_| TcpStream::connect(addr).unwrap()));
    let answer = send(addr, HEALTH);
    assert_eq!(status(&answer), "HTTP/1.1 503");
    // The clients close, the workers see the end of each stream, and the queue empties.
    drop(silent);
    let begin = Instant::now();
    while status(&send(addr, HEALTH)) != "HTTP/1.1 200" {
        assert!(begin.elapsed() < LONG, "the server did not recover");
        thread::sleep(Duration::from_millis(20));
    }
}

/// The largest statements that fit in the body do not overflow the stack of a worker.
/// A stack overflow stops the whole process, so the test process would stop here.
#[test]
fn largest_statements_do_not_stop_the_server() {
    let addr = start("", LONG);
    let sum = format!("SELECT {}1", "1+".repeat(996));
    assert_eq!(sum.len(), 2000);
    assert_eq!(body(&post(addr, &sum)), r#"{"ok":true,"rows":[["997"]]}"#);
    for sql in [
        format!("SELECT {}1{}", "(".repeat(996), ")".repeat(996)),
        format!("SELECT {}1", "- ".repeat(996)),
        format!("SELECT {}1", "NOT ".repeat(498)),
        format!("SELECT {}1", "CASE WHEN ".repeat(199)),
        format!("SELECT {}1", "1,".repeat(996)),
    ] {
        let answer = post(addr, &sql);
        assert_eq!(status(&answer), "HTTP/1.1 200");
        assert!(body(&answer).starts_with(r#"{"ok":"#), "{answer}");
    }
    assert_eq!(status(&send(addr, HEALTH)), "HTTP/1.1 200");
}

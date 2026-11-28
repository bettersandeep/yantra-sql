//! The lekha-server binary. `README.md` has the environment variables.

#![forbid(unsafe_code)]

use std::env;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::process::exit;
use std::time::Duration;

use lekha_server::{Config, serve};

fn main() {
    // A panic message can hold text from a request. Print only the place.
    std::panic::set_hook(Box::new(|info| {
        if let Some(place) = info.location() {
            let _ = writeln!(std::io::stderr(), "lekha-server: panic at {place}");
        }
    }));
    let addr = env::var("LEKHA_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let addr: SocketAddr = match addr.parse() {
        Ok(addr) => addr,
        Err(error) => {
            eprintln!("lekha-server: LEKHA_ADDR {addr:?}: {error}");
            exit(2);
        }
    };
    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("lekha-server: cannot listen on {addr}: {error}");
            exit(1);
        }
    };
    println!(
        "lekha-server: listening on {addr}, engine lekha {}",
        lekha::VERSION
    );
    let config = Config {
        allow_origin: env::var("LEKHA_ALLOW_ORIGIN").unwrap_or_default(),
        timeout: Duration::from_secs(5),
        log: true,
    };
    serve(listener, config);
}

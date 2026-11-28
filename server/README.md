# lekha-server

A small HTTP server for the lekha SQL engine.
A client sends one SQL statement and gets the answer as JSON.
The engine has no files and no tables, so a request cannot read or change data.

The crate uses the engine and the Rust standard library, and nothing else.
It forbids `unsafe` code.
It is not part of the engine crate: the root `Cargo.toml` and the root `Cargo.lock` do not know it.

## Build and run

From the root of the repo:

```
cargo build --release --locked --manifest-path server/Cargo.toml
```

The binary is `server/target/release/lekha-server`.

```
LEKHA_ADDR=127.0.0.1:8080 server/target/release/lekha-server
```

The server prints one line at the start, with the address and the version of the engine.
Then it prints one line per request: the method, the path, the status, the bytes of the body, and the milliseconds.
A request that the server refuses before it routes it has `- -` as the method and the path.
The log never has the SQL text.

`lekha-server.service` is a systemd unit for a Raspberry Pi.
It expects the binary at `/usr/local/bin/lekha-server`.
The unit is not tested.

## Environment variables

| Name | Default | Meaning |
| --- | --- | --- |
| `LEKHA_ADDR` | `127.0.0.1:8080` | The IP address and the port to listen on. A host name is not accepted. |
| `LEKHA_ALLOW_ORIGIN` | empty | When set, each response has `Access-Control-Allow-Origin` with this exact value. When empty, no response has a CORS header. |

An address that does not parse stops the server with exit code 2.
An address that the server cannot listen on stops it with exit code 1.

## Routes

`GET /` returns three lines of text that say how to call the server.

```
curl http://127.0.0.1:8080/
```

`GET /health` returns `{"ok":true,"engine":"lekha","version":"0.0.0"}`.

```
curl http://127.0.0.1:8080/health
```

`POST /query` runs the body as one SQL statement.

```
curl --data-binary 'SELECT 7 / 2, 1 / 0' http://127.0.0.1:8080/query
```

The answer is `{"ok":true,"rows":[["3",null]]}`.
Each value is a JSON string or `null`.
An integer is its decimal text.
When the engine returns an error, the status is still 200 and the body is `{"ok":false,"error":"..."}`.
A real number is the text that SQLite gives for `CAST(<real> AS TEXT)`: `1.0`, `3.5`, `0.333333333333333`, `9.22337203685478e+18`.
`SELECT CAST(7 AS REAL) / 2, AVG(1)` gives `{"ok":true,"rows":[["3.5","1.0"]]}`.
The function `lekha::real_text` of the engine makes this text.

`OPTIONS /query` is the CORS preflight, and returns 204.
When `LEKHA_ALLOW_ORIGIN` is set, the response has `Access-Control-Allow-Methods`, `Access-Control-Allow-Headers` and `Access-Control-Max-Age`.

```
curl -i -X OPTIONS http://127.0.0.1:8080/query
```

Each other path returns 404.
A known path with a wrong method returns 405 with an `Allow` header.
A query string is ignored: `/health?a=1` is `/health`.
The body of each error status is `{"ok":false,"error":"..."}` with the name of the status.

## Limits

- One request per connection. Each response has `Connection: close`.
- 4 worker threads and a queue of 16 connections. When the queue is full, the answer is 503.
- 5 seconds to send the full request, and 5 seconds for each write. A request that is not complete in time gets 408.
- 8 KB for the request line and the headers. More gives 431.
- 2,000 bytes for the body. More gives 413.
- `Content-Length` is necessary on `POST`. A request without it, or with `Transfer-Encoding`, gets 411.
- The body must be UTF-8. Other bytes give 400, and so does each malformed request.
- No time limit for one statement. The engine limits the nesting and the size of an expression, so a statement of 2,000 bytes cannot run long.
- When the server refuses a request before it reads all of it, it closes the connection. A client that is still sending can see a reset connection and not the status.

## What it does not do

- No TLS. A tunnel or a proxy in front of the server does that.
- No authentication. Each client that can connect can run a statement.
- No rate limit. The proxy in front must do it. Four slow clients can hold the four workers for 5 seconds each.
- No keep-alive, no chunked body, no compression.

## Tests

```
docker run --rm --network none -v "$PWD":/src:ro yantra-rust:1.96.1 sh -c 'cp -r /src /tmp/w && cd /tmp/w/server && cargo fmt --check && cargo clippy --locked --offline --all-targets -- -D warnings && cargo test --locked --offline'
```

Run this from the root of the repo.
`yantra/README.md` says how to make the image `yantra-rust:1.96.1`.
The tests in `tests/http.rs` start the server on a free port of `127.0.0.1` and talk to it with a TCP stream.
`cargo test -- --nocapture` prints each request and its raw response.
The CI of the repo does not run these tests.

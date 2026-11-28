# site

The demo site of lekha.
It has two parts: the score map and the playground page.
The workflow `.github/workflows/pages.yml` builds the site and publishes it with GitHub Pages on each push to `main`.

## Files

- `make_map.py`: draws the score map.
  Python 3, standard library only.
- `test_make_map.py`: the test of `make_map.py`.
- `wasm/`: a separate crate that compiles the engine to WebAssembly.
- `index.html`, `site.css`, `site.js`: the page.
- `build.sh`: builds the folder `dist/`, which is ready to publish.
- `Dockerfile`: the image for the wasm build.

git ignores each output: `slt-map.tsv`, `score-map.svg`, `score.json`, `dist/` and `wasm/target/`.
The outputs are not in the repo, because a copy in the repo would be wrong after the next change of the score.

## The score map

The map has one square for each applicable test record, in the order of the run.
A green square is a record that passes.
An amber square is a record that fails.
The map comes from a real run of the test harness.

Run the harness with `SLT_MAP`, then draw the map.
The two commands use no network.

```
docker run --rm --network none -v "$PWD":/src:ro -v "$PWD/site":/out yantra-rust:1.96.1 sh -c 'cp -r /src /tmp/w && cd /tmp/w && SLT_MAP=/out/slt-map.tsv cargo test --locked --offline --test slt'
docker run --rm --network none -v "$PWD/site":/site python:3.13-slim python3 /site/make_map.py /site/slt-map.tsv "$(git rev-parse HEAD)"
```

The first command writes `site/slt-map.tsv`.
Each line is `<file>`, `<line>` and `P` or `F`, with a tab between them.
The second command writes `site/score-map.svg` and `site/score.json`.
The same input gives the same output, byte for byte.

## The playground page

The page runs a SQL statement in the real engine, in the browser.
The engine is `lekha.wasm`.
The page uses no server, no script of a different site, no font from the network and no tracking.

The engine crate forbids `unsafe` and has no dependency.
So `wasm/` is a separate crate with its own `Cargo.lock`.
It depends only on the engine, by path.
It exports three functions by hand: `alloc`, `dealloc` and `run`.
The `unsafe` code for the shared memory is only in `wasm/src/lib.rs`.
`run` takes SQL of 2,000 bytes at most and returns JSON: `{"ok":true,"rows":[["3",null]]}` or `{"ok":false,"error":"..."}`.
Each value is a string, so a large integer keeps each digit, and NULL is `null`.
A panic of the engine is a trap in WebAssembly.
`site.js` catches the trap, shows an error, and makes a new instance for the next run.

## Build

Make the image one time, with a network.
It downloads the Rust standard library for the target `wasm32-unknown-unknown`.

```
docker build -t lekha-wasm:1.96.1 - < site/Dockerfile
```

Make the score map first.
Then build `site/dist` with no network:

```
docker run --rm --network none -v "$PWD":/src:ro -v "$PWD/site/dist":/out lekha-wasm:1.96.1 sh -c 'cp -r /src /tmp/w && cd /tmp/w && sh site/build.sh && cp site/dist/* /out/'
```

Look at the result:

```
python3 -m http.server 8000 --bind 127.0.0.1 --directory site/dist
```

Then open `http://127.0.0.1:8000/`.
The page needs a web server.
From a `file:` address, most browsers do not let the page read `lekha.wasm`.

## Tests

```
python3 -m unittest discover -s site -p 'test_*.py'
docker run --rm --network none -v "$PWD":/src:ro yantra-rust:1.96.1 sh -c 'cp -r /src /tmp/w && cd /tmp/w/site/wasm && cargo fmt --check && cargo clippy --locked --offline --all-targets -- -D warnings && cargo test --locked --offline'
```

The page has a test hook.
Each `?q=<sql>` in the address runs on load, in order.
For example, `http://127.0.0.1:8000/?q=SELECT%201%20%2F%200` shows `NULL`.

`ci.yml` does not build `wasm/`.
A change of the engine API that breaks `wasm/` shows as a failed `pages` workflow after the merge.

## GitHub Pages

A person must switch on one setting of the repo, one time: Settings, Pages, Build and deployment, Source: "GitHub Actions".
Before that, the `deploy` job of the workflow fails.

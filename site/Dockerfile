# The image for the wasm build: the image of the build checks, plus the wasm target.
# Build it one time, with a network. It downloads the Rust standard library for wasm32-unknown-unknown:
#   docker build -t lekha-wasm:1.96.1 - < site/Dockerfile
FROM yantra-rust:1.96.1
RUN rustup target add wasm32-unknown-unknown

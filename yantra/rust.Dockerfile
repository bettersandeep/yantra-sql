# The image for the build checks: the pinned Rust version with rustfmt and clippy.
# Build it one time, with a network:
#   docker build -t yantra-rust:1.96.1 -f yantra/rust.Dockerfile yantra
FROM rust:1.96.1-slim
RUN rustup component add rustfmt clippy

# syntax=docker/dockerfile:1

# Builder: compile the fpv CLI from source.
FROM rust:1-slim-bookworm AS builder
WORKDIR /build

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p fpv-cli && \
    cp target/release/fpv /build/fpv

# Runtime: minimal image with fpv plus the FFmpeg/FFprobe it shells out to.
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ffmpeg ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/fpv /usr/local/bin/fpv

WORKDIR /work

ENTRYPOINT ["fpv"]
CMD ["--help"]

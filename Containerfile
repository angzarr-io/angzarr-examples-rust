# syntax=docker/dockerfile:1.4
# Rust poker examples - standalone repo build
#
# Build single image:
#   docker build --target agg-player -t ghcr.io/angzarr-io/examples-rust-agg-player .
#
# Build all:
#   for target in agg-player agg-table agg-hand saga-table-hand saga-table-player saga-hand-table saga-hand-player pmg-hand-flow prj-output; do
#     docker build --target $target -t ghcr.io/angzarr-io/examples-rust-$target .
#   done

ARG RUST_VERSION=1.86

# ============================================================================
# Builder - fetch deps and compile
# ============================================================================
FROM docker.io/library/rust:${RUST_VERSION}-alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    protobuf-dev \
    protoc \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    curl

# Install buf for proto export
RUN curl -sSL https://github.com/bufbuild/buf/releases/latest/download/buf-Linux-x86_64 -o /usr/local/bin/buf && \
    chmod +x /usr/local/bin/buf

RUN rustup target add x86_64-unknown-linux-musl

ENV RUSTFLAGS="-C target-feature=+crt-static"
ENV OPENSSL_STATIC=1
ENV OPENSSL_DIR=/usr

WORKDIR /app

# Export example protos from buf registry
ENV EXAMPLES_PROTO_ROOT=/app/examples-proto
RUN buf export buf.build/angzarr/examples -o /app/examples-proto

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY proto/Cargo.toml ./proto/
COPY player/agg/Cargo.toml ./player/agg/
COPY player/agg-oo/Cargo.toml ./player/agg-oo/
COPY player/upc/Cargo.toml ./player/upc/
COPY table/agg/Cargo.toml ./table/agg/
COPY table/agg-oo/Cargo.toml ./table/agg-oo/
COPY table/saga-hand/Cargo.toml ./table/saga-hand/
COPY table/saga-hand-oo/Cargo.toml ./table/saga-hand-oo/
COPY table/saga-player/Cargo.toml ./table/saga-player/
COPY hand/agg/Cargo.toml ./hand/agg/
COPY hand/saga-table/Cargo.toml ./hand/saga-table/
COPY hand/saga-player/Cargo.toml ./hand/saga-player/
COPY pmg-hand-flow/Cargo.toml ./pmg-hand-flow/
COPY prj-output/Cargo.toml ./prj-output/
COPY prj-output-oo/Cargo.toml ./prj-output-oo/
COPY tests/Cargo.toml ./tests/

# Create stub files for dependency caching
RUN mkdir -p proto/src \
    player/agg/src player/agg-oo/src player/upc/src \
    table/agg/src table/agg-oo/src table/saga-hand/src table/saga-hand-oo/src table/saga-player/src \
    hand/agg/src hand/saga-table/src hand/saga-player/src \
    pmg-hand-flow/src prj-output/src prj-output-oo/src \
    tests/tests && \
    echo "fn main() {}" > proto/src/lib.rs && \
    echo "fn main() {}" > player/agg/src/main.rs && \
    echo "fn main() {}" > player/agg-oo/src/main.rs && \
    echo "fn main() {}" > player/upc/src/main.rs && \
    echo "fn main() {}" > table/agg/src/main.rs && \
    echo "fn main() {}" > table/agg-oo/src/main.rs && \
    echo "fn main() {}" > table/saga-hand/src/main.rs && \
    echo "fn main() {}" > table/saga-hand-oo/src/main.rs && \
    echo "fn main() {}" > table/saga-player/src/main.rs && \
    echo "fn main() {}" > hand/agg/src/main.rs && \
    echo "fn main() {}" > hand/saga-table/src/main.rs && \
    echo "fn main() {}" > hand/saga-player/src/main.rs && \
    echo "fn main() {}" > pmg-hand-flow/src/main.rs && \
    echo "fn main() {}" > prj-output/src/main.rs && \
    echo "fn main() {}" > prj-output-oo/src/main.rs && \
    echo "fn main() {}" > tests/tests/player.rs && \
    echo "fn main() {}" > tests/tests/table.rs && \
    echo "fn main() {}" > tests/tests/hand.rs

# Copy proto build.rs (needed to generate proto code)
COPY proto/build.rs ./proto/

# Build dependencies only
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl --workspace 2>/dev/null || true

# Copy real source
COPY proto/src ./proto/src
COPY player/agg/src ./player/agg/src
COPY player/upc/src ./player/upc/src
COPY table/agg/src ./table/agg/src
COPY table/saga-hand/src ./table/saga-hand/src
COPY table/saga-player/src ./table/saga-player/src
COPY hand/agg/src ./hand/agg/src
COPY hand/saga-table/src ./hand/saga-table/src
COPY hand/saga-player/src ./hand/saga-player/src
COPY pmg-hand-flow/src ./pmg-hand-flow/src
COPY prj-output/src ./prj-output/src
COPY tests/tests ./tests/tests

# Build all binaries
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl --workspace && \
    mkdir -p /out && \
    cp target/x86_64-unknown-linux-musl/release/agg-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/upc-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/agg-table /out/ && \
    cp target/x86_64-unknown-linux-musl/release/agg-hand /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-table-hand /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-table-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-hand-table /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-hand-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/pmg-hand-flow /out/ && \
    cp target/x86_64-unknown-linux-musl/release/prj-output /out/

# ============================================================================
# Runtime base - minimal distroless
# ============================================================================
FROM gcr.io/distroless/static-debian12:nonroot AS runtime
WORKDIR /app
USER nonroot:nonroot
ENV RUST_LOG=info

# ============================================================================
# Individual service images
# ============================================================================
FROM runtime AS agg-player
COPY --from=builder --chown=nonroot:nonroot /out/agg-player ./server
ENV PORT=50001
EXPOSE 50001
ENTRYPOINT ["./server"]

FROM runtime AS agg-table
COPY --from=builder --chown=nonroot:nonroot /out/agg-table ./server
ENV PORT=50002
EXPOSE 50002
ENTRYPOINT ["./server"]

FROM runtime AS agg-hand
COPY --from=builder --chown=nonroot:nonroot /out/agg-hand ./server
ENV PORT=50003
EXPOSE 50003
ENTRYPOINT ["./server"]

FROM runtime AS saga-table-hand
COPY --from=builder --chown=nonroot:nonroot /out/saga-table-hand ./server
ENV PORT=50011
EXPOSE 50011
ENTRYPOINT ["./server"]

FROM runtime AS saga-table-player
COPY --from=builder --chown=nonroot:nonroot /out/saga-table-player ./server
ENV PORT=50012
EXPOSE 50012
ENTRYPOINT ["./server"]

FROM runtime AS saga-hand-table
COPY --from=builder --chown=nonroot:nonroot /out/saga-hand-table ./server
ENV PORT=50013
EXPOSE 50013
ENTRYPOINT ["./server"]

FROM runtime AS saga-hand-player
COPY --from=builder --chown=nonroot:nonroot /out/saga-hand-player ./server
ENV PORT=50014
EXPOSE 50014
ENTRYPOINT ["./server"]

FROM runtime AS pmg-hand-flow
COPY --from=builder --chown=nonroot:nonroot /out/pmg-hand-flow ./server
ENV PORT=50020
EXPOSE 50020
ENTRYPOINT ["./server"]

FROM runtime AS prj-output
COPY --from=builder --chown=nonroot:nonroot /out/prj-output ./server
ENV PORT=50030
EXPOSE 50030
ENTRYPOINT ["./server"]

FROM runtime AS upc-player
COPY --from=builder --chown=nonroot:nonroot /out/upc-player ./server
ENV PORT=50040
EXPOSE 50040
ENTRYPOINT ["./server"]

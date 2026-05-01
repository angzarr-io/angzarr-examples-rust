# syntax=docker/dockerfile:1.4
# Rust poker examples - self-contained repo build
#
# Build single image:
#   docker build --target agg-player -t ghcr.io/angzarr-io/examples-rust-agg-player .
#
# Build all:
#   for target in agg-player agg-table agg-hand saga-table-hand saga-table-player saga-hand-table saga-hand-player pmg-hand-flow prj-output; do
#     docker build --target $target -t ghcr.io/angzarr-io/examples-rust-$target .
#   done

ARG RUST_VERSION=1.87

# ============================================================================
# Proto generation stage - runs build.rs to generate proto code
# ============================================================================
FROM docker.io/library/rust:${RUST_VERSION}-alpine AS proto-gen

RUN apk add --no-cache musl-dev protobuf-dev protoc openssl-dev openssl-libs-static pkgconfig

RUN rustup target add x86_64-unknown-linux-musl

ENV RUSTFLAGS="-C target-feature=+crt-static"
ENV OPENSSL_STATIC=1
ENV OPENSSL_DIR=/usr

WORKDIR /app

# Stage example protos from the angzarr-project submodule. Mirrors what
# `justfile.container::proto-gen` does on the host: the canonical files live
# at `angzarr_client/proto/examples/X.proto` (matching `package
# angzarr_client.proto.examples`), but the rust build.rs include root is
# flat (`examples/X.proto`). Copy + rewrite the imports.
ENV EXAMPLES_PROTO_ROOT=/app/examples-proto
COPY angzarr-project/ ./angzarr-project/
RUN mkdir -p /app/examples-proto/examples && \
    cp angzarr-project/proto/angzarr_client/proto/examples/*.proto /app/examples-proto/examples/ && \
    sed -i 's|angzarr_client/proto/examples/|examples/|g' /app/examples-proto/examples/*.proto && \
    ls -la /app/examples-proto/examples/

# Copy what's needed for proto generation
COPY Cargo.toml Cargo.lock ./
COPY proto/ ./proto/
COPY angzarr-client-rust/ ./angzarr-client-rust/

# Create minimal stubs
RUN mkdir -p examples-utils/src \
    player/agg/src player/upc/src player/saga-table/src \
    reservation/agg/src \
    table/agg/src table/saga-hand/src table/saga-player/src \
    hand/agg/src hand/upc/src hand/saga-table/src hand/saga-player/src \
    hand-flow-oo/src tournament/agg/src \
    pmg-hand-flow/src pmg-reservation/src \
    prj-output/src tests/tests && \
    for d in examples-utils \
             player/agg player/upc player/saga-table reservation/agg \
             table/agg table/saga-hand table/saga-player \
             hand/agg hand/upc hand/saga-table hand/saga-player \
             hand-flow-oo tournament/agg \
             pmg-hand-flow pmg-reservation prj-output; do \
      echo "[package]\nname = \"stub\"\nversion = \"0.1.0\"\nedition = \"2021\"" > $d/Cargo.toml 2>/dev/null || true; \
      echo "fn main() {}" > $d/src/main.rs; \
      : > $d/src/lib.rs; \
    done && \
    for t in player table hand orchestration saga process_manager projector betting_round game_rules raise_tracking tournament poker_game_unit acceptance; do echo "fn main() {}" > tests/tests/$t.rs; done

# Copy real Cargo.toml files
COPY examples-utils/Cargo.toml ./examples-utils/
COPY player/agg/Cargo.toml ./player/agg/
COPY player/upc/Cargo.toml ./player/upc/
COPY player/saga-table/Cargo.toml ./player/saga-table/
COPY reservation/agg/Cargo.toml ./reservation/agg/
COPY table/agg/Cargo.toml ./table/agg/
COPY table/saga-hand/Cargo.toml ./table/saga-hand/
COPY table/saga-player/Cargo.toml ./table/saga-player/
COPY hand/agg/Cargo.toml ./hand/agg/
COPY hand/upc/Cargo.toml ./hand/upc/
COPY hand/saga-table/Cargo.toml ./hand/saga-table/
COPY hand/saga-player/Cargo.toml ./hand/saga-player/
COPY hand-flow-oo/Cargo.toml ./hand-flow-oo/
COPY tournament/agg/Cargo.toml ./tournament/agg/
COPY pmg-hand-flow/Cargo.toml ./pmg-hand-flow/
COPY pmg-reservation/Cargo.toml ./pmg-reservation/
COPY prj-output/Cargo.toml ./prj-output/
COPY tests/Cargo.toml ./tests/

# Run cargo build to execute proto build.rs
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl -p examples-proto 2>&1 || true

# Extract generated proto files
RUN mkdir -p /proto-out && \
    cp -r target/x86_64-unknown-linux-musl/release/build/examples-proto-*/out/* /proto-out/ 2>/dev/null || true

# ============================================================================
# Builder deps - compile dependencies only
# ============================================================================
FROM docker.io/library/rust:${RUST_VERSION}-alpine AS builder-deps

RUN apk add --no-cache \
    musl-dev \
    protobuf-dev \
    protoc \
    openssl-dev \
    openssl-libs-static \
    pkgconfig

RUN rustup target add x86_64-unknown-linux-musl

ENV RUSTFLAGS="-C target-feature=+crt-static"
ENV OPENSSL_STATIC=1
ENV OPENSSL_DIR=/usr

WORKDIR /app

# Stage example protos from the angzarr-project submodule (same approach
# as the proto-gen stage).
ENV EXAMPLES_PROTO_ROOT=/app/examples-proto
COPY angzarr-project/ ./angzarr-project/
RUN mkdir -p /app/examples-proto/examples && \
    cp angzarr-project/proto/angzarr_client/proto/examples/*.proto /app/examples-proto/examples/ && \
    sed -i 's|angzarr_client/proto/examples/|examples/|g' /app/examples-proto/examples/*.proto && \
    ls -la /app/examples-proto/examples/

# Copy pre-generated proto files
COPY --from=proto-gen /proto-out/ /proto-cache/

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY proto/ ./proto/
COPY angzarr-client-rust/ ./angzarr-client-rust/
COPY examples-utils/Cargo.toml ./examples-utils/
COPY player/agg/Cargo.toml ./player/agg/
COPY player/upc/Cargo.toml ./player/upc/
COPY player/saga-table/Cargo.toml ./player/saga-table/
COPY reservation/agg/Cargo.toml ./reservation/agg/
COPY table/agg/Cargo.toml ./table/agg/
COPY table/saga-hand/Cargo.toml ./table/saga-hand/
COPY table/saga-player/Cargo.toml ./table/saga-player/
COPY hand/agg/Cargo.toml ./hand/agg/
COPY hand/upc/Cargo.toml ./hand/upc/
COPY hand/saga-table/Cargo.toml ./hand/saga-table/
COPY hand/saga-player/Cargo.toml ./hand/saga-player/
COPY hand-flow-oo/Cargo.toml ./hand-flow-oo/
COPY tournament/agg/Cargo.toml ./tournament/agg/
COPY pmg-hand-flow/Cargo.toml ./pmg-hand-flow/
COPY pmg-reservation/Cargo.toml ./pmg-reservation/
COPY prj-output/Cargo.toml ./prj-output/
COPY tests/Cargo.toml ./tests/

# Create stubs
RUN mkdir -p examples-utils/src \
    player/agg/src player/upc/src player/saga-table/src \
    reservation/agg/src \
    table/agg/src table/saga-hand/src table/saga-player/src \
    hand/agg/src hand/upc/src hand/saga-table/src hand/saga-player/src \
    hand-flow-oo/src tournament/agg/src \
    pmg-hand-flow/src pmg-reservation/src \
    prj-output/src tests/src tests/tests && \
    echo "fn main() {}" > proto/src/lib.rs && \
    echo "" > tests/src/lib.rs && \
    : > examples-utils/src/lib.rs && \
    for d in player/agg player/upc reservation/agg \
             table/agg table/saga-hand table/saga-player \
             hand/agg hand/upc hand/saga-table hand/saga-player \
             hand-flow-oo tournament/agg \
             pmg-hand-flow pmg-reservation prj-output; do \
      echo "fn main() {}" > $d/src/main.rs; \
      : > $d/src/lib.rs; \
    done && \
    : > player/saga-table/src/lib.rs && \
    for t in player table hand orchestration saga process_manager projector betting_round game_rules raise_tracking tournament poker_game_unit acceptance; do echo "fn main() {}" > tests/tests/$t.rs; done

# Build dependencies
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl --workspace 2>&1 || true

# ============================================================================
# Builder - compile with real source
# ============================================================================
FROM builder-deps AS builder

# Remove stubs
RUN rm -rf proto/src examples-utils/src player/*/src reservation/*/src \
    table/*/src hand/*/src tournament/*/src \
    pmg-hand-flow/src pmg-reservation/src \
    prj-output/src tests/tests

# Copy real source
COPY proto/src ./proto/src
COPY examples-utils/src ./examples-utils/src
COPY player/agg/src ./player/agg/src
COPY player/upc/src ./player/upc/src
COPY player/saga-table/src ./player/saga-table/src
COPY reservation/agg/src ./reservation/agg/src
COPY table/agg/src ./table/agg/src
COPY table/saga-hand/src ./table/saga-hand/src
COPY table/saga-player/src ./table/saga-player/src
COPY hand/agg/src ./hand/agg/src
COPY hand/upc/src ./hand/upc/src
COPY hand/saga-table/src ./hand/saga-table/src
COPY hand/saga-player/src ./hand/saga-player/src
COPY hand-flow-oo/src ./hand-flow-oo/src
COPY tournament/agg/src ./tournament/agg/src
COPY pmg-hand-flow/src ./pmg-hand-flow/src
COPY pmg-reservation/src ./pmg-reservation/src
COPY prj-output/src ./prj-output/src
COPY tests/src ./tests/src
COPY tests/tests ./tests/tests

# Inject pre-generated proto files
RUN BUILD_DIR=$(ls -d target/x86_64-unknown-linux-musl/release/build/examples-proto-*/out 2>/dev/null | head -1) && \
    if [ -n "$BUILD_DIR" ]; then \
        cp -r /proto-cache/* "$BUILD_DIR/" 2>/dev/null || true; \
    fi

# Clean workspace crate artifacts to force rebuild from real source
# (the builder-deps stage built against empty stub lib.rs files).
RUN rm -rf target/x86_64-unknown-linux-musl/release/.fingerprint/agg-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/saga-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/pmg-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/prj-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/upc-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/hand-flow-oo-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/examples-proto-* \
    target/x86_64-unknown-linux-musl/release/.fingerprint/examples-utils-* \
    target/x86_64-unknown-linux-musl/release/deps/libagg* \
    target/x86_64-unknown-linux-musl/release/deps/libsaga* \
    target/x86_64-unknown-linux-musl/release/deps/libpmg* \
    target/x86_64-unknown-linux-musl/release/deps/libprj* \
    target/x86_64-unknown-linux-musl/release/deps/libupc* \
    target/x86_64-unknown-linux-musl/release/deps/libhand_flow_oo* \
    target/x86_64-unknown-linux-musl/release/deps/libexamples_proto* \
    target/x86_64-unknown-linux-musl/release/deps/libexamples_utils*

# Build all binaries
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl --workspace && \
    mkdir -p /out && \
    cp target/x86_64-unknown-linux-musl/release/agg-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/upc-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/upc-hand /out/ && \
    cp target/x86_64-unknown-linux-musl/release/agg-table /out/ && \
    cp target/x86_64-unknown-linux-musl/release/agg-hand /out/ && \
    cp target/x86_64-unknown-linux-musl/release/agg-tournament /out/ && \
    cp target/x86_64-unknown-linux-musl/release/agg-reservation /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-table-hand /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-table-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-hand-table /out/ && \
    cp target/x86_64-unknown-linux-musl/release/saga-hand-player /out/ && \
    cp target/x86_64-unknown-linux-musl/release/pmg-hand-flow /out/ && \
    cp target/x86_64-unknown-linux-musl/release/pmg-reservation /out/ && \
    cp target/x86_64-unknown-linux-musl/release/hand-flow-oo /out/ && \
    cp target/x86_64-unknown-linux-musl/release/prj-pretty-output /out/prj-output

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

FROM runtime AS agg-tournament
COPY --from=builder --chown=nonroot:nonroot /out/agg-tournament ./server
ENV PORT=50004
EXPOSE 50004
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
ENV PORT=50391
EXPOSE 50391
ENTRYPOINT ["./server"]

FROM runtime AS pmg-reservation
COPY --from=builder --chown=nonroot:nonroot /out/pmg-reservation ./server
ENV PORT=50392
EXPOSE 50392
ENTRYPOINT ["./server"]

FROM runtime AS agg-reservation
COPY --from=builder --chown=nonroot:nonroot /out/agg-reservation ./server
ENV PORT=50305
EXPOSE 50305
ENTRYPOINT ["./server"]

FROM runtime AS upc-hand
COPY --from=builder --chown=nonroot:nonroot /out/upc-hand ./server
ENV PORT=50041
EXPOSE 50041
ENTRYPOINT ["./server"]

FROM runtime AS hand-flow-oo
COPY --from=builder --chown=nonroot:nonroot /out/hand-flow-oo ./server
ENV PORT=50395
EXPOSE 50395
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

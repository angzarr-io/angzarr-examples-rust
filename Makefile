# Makefile for examples-rust
#
# Prerequisites:
#   - buf CLI: https://buf.build/docs/installation
#   - Rust toolchain: https://rustup.rs/

# Proto version from BSR (without 'v' prefix)
PROTO_VERSION ?= 0.1.2

.PHONY: protos build test clean help

help:
	@echo "Available targets:"
	@echo "  protos  - Fetch example protos from buf.build/angzarr/examples"
	@echo "  build   - Build all crates (runs protos first)"
	@echo "  test    - Run all tests (runs protos first)"
	@echo "  clean   - Remove build artifacts and exported protos"

protos:
	@echo "Fetching protos from buf.build/angzarr/examples:$(PROTO_VERSION)..."
	buf export buf.build/angzarr/examples:$(PROTO_VERSION) -o examples-proto
	@echo "Protos exported to examples-proto/"

build: protos
	cargo build

test: protos
	cargo test

clean:
	cargo clean
	rm -rf examples-proto/

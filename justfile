# Rust poker examples
#
# Container Overlay Pattern:
# --------------------------
# This justfile uses an overlay pattern for container execution:
#
# 1. `justfile` (this file) - runs on the host, delegates to container
# 2. `justfile.container` - mounted over this file inside the container
#
# When running outside a devcontainer:
#   - Builds/uses local devcontainer image with `just` pre-installed
#   - Podman mounts justfile.container as /workspace/justfile
#
# When running inside a devcontainer (DEVCONTAINER=true):
#   - Commands execute directly via `just <target>`
#   - No container nesting

set shell := ["bash", "-c"]

ROOT := `git rev-parse --show-toplevel`
IMAGE := "angzarr-examples-rust-dev"

# Build the devcontainer image
[private]
_build-image:
    podman build --network=host -t {{IMAGE}} -f "{{ROOT}}/.devcontainer/Containerfile" "{{ROOT}}/.devcontainer"

# Run just target in container (or directly if already in devcontainer)
[private]
_container +ARGS: _build-image
    #!/usr/bin/env bash
    if [ "${DEVCONTAINER:-}" = "true" ]; then
        just {{ARGS}}
    else
        podman run --rm --network=host \
            -v "{{ROOT}}:/workspace:Z" \
            -v "{{ROOT}}/justfile.container:/workspace/justfile:ro" \
            -w /workspace \
            -e CARGO_HOME=/workspace/.cargo-container \
            {{IMAGE}} just {{ARGS}}
    fi

# Default: list available commands
[no-exit-message]
default:
    @just --list

# Build all poker aggregates (release)
build:
    just _container build

# Build all poker aggregates (debug)
build-dev:
    just _container build-dev

# Run unit tests
test-unit:
    just _container test-unit

# Run acceptance/BDD tests
test-acceptance:
    just _container test-acceptance

# Run all tests (unit + acceptance)
test:
    just _container test

# Check code compiles
check:
    just _container check

# Format code
fmt:
    just _container fmt

# Lint code
lint:
    just _container lint

# Clean build artifacts
clean:
    just _container clean

# Run poker in standalone mode (player:50001, table:50002, hand:50003)
run:
    just _container run

# Run poker in standalone mode (debug build)
run-dev:
    just _container run-dev

# =============================================================================
# Kind Cluster Management (runs on host, not in container)
# =============================================================================

CLUSTER_NAME := "angzarr-test"

# Create kind cluster for acceptance tests
kind-create:
    #!/usr/bin/env bash
    set -euo pipefail
    if kind get clusters 2>/dev/null | grep -q "^{{CLUSTER_NAME}}$"; then
        echo "Cluster {{CLUSTER_NAME}} already exists"
    else
        kind create cluster --config deploy/kind/cluster.yaml --name {{CLUSTER_NAME}}
    fi

# Delete kind cluster
kind-delete:
    kind delete cluster --name {{CLUSTER_NAME}} || true

# Load locally built images into kind (for local testing)
kind-load-images:
    #!/usr/bin/env bash
    set -euo pipefail
    images=(
        "ghcr.io/angzarr-io/examples-rust-agg-player:latest"
        "ghcr.io/angzarr-io/examples-rust-agg-table:latest"
        "ghcr.io/angzarr-io/examples-rust-agg-hand:latest"
        "ghcr.io/angzarr-io/examples-rust-saga-table-hand:latest"
        "ghcr.io/angzarr-io/examples-rust-saga-hand-player:latest"
        "ghcr.io/angzarr-io/examples-rust-prj-output:latest"
    )
    for img in "${images[@]}"; do
        if docker image inspect "$img" &>/dev/null; then
            kind load docker-image "$img" --name {{CLUSTER_NAME}}
        fi
    done

# Deploy infrastructure (postgres, rabbitmq)
deploy-infra:
    #!/usr/bin/env bash
    set -euo pipefail
    kubectl apply -k deploy/k8s/base --selector='app in (postgres,rabbitmq)' || \
        kubectl apply -f deploy/k8s/base/namespace.yaml && \
        kubectl apply -f deploy/k8s/base/config.yaml && \
        kubectl apply -f deploy/k8s/base/postgres.yaml && \
        kubectl apply -f deploy/k8s/base/rabbitmq.yaml
    echo "Waiting for postgres..."
    kubectl wait --for=condition=ready pod -l app=postgres -n angzarr-test --timeout=120s
    echo "Waiting for rabbitmq..."
    kubectl wait --for=condition=ready pod -l app=rabbitmq -n angzarr-test --timeout=180s

# Deploy poker applications
deploy-apps tag="latest":
    #!/usr/bin/env bash
    set -euo pipefail
    cd deploy/k8s/overlays/ci
    # Set image tags
    kustomize edit set image \
        ghcr.io/angzarr-io/examples-rust-agg-player:{{tag}} \
        ghcr.io/angzarr-io/examples-rust-agg-table:{{tag}} \
        ghcr.io/angzarr-io/examples-rust-agg-hand:{{tag}} \
        ghcr.io/angzarr-io/examples-rust-saga-table-hand:{{tag}} \
        ghcr.io/angzarr-io/examples-rust-saga-hand-player:{{tag}} \
        ghcr.io/angzarr-io/examples-rust-prj-output:{{tag}}
    cd -
    kubectl apply -k deploy/k8s/overlays/ci
    echo "Waiting for poker apps..."
    kubectl wait --for=condition=ready pod -l app.kubernetes.io/part-of=angzarr-poker-test -n angzarr-test --timeout=180s || true

# Deploy everything to kind
deploy-all tag="latest": deploy-infra (deploy-apps tag)

# Run acceptance tests against deployed cluster
test-e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    # Port-forward gateway for tests
    kubectl port-forward -n angzarr-test svc/gateway 9084:9084 &
    PF_PID=$!
    trap "kill $PF_PID 2>/dev/null || true" EXIT
    sleep 2
    # Run acceptance tests
    export GATEWAY_URL="http://localhost:9084"
    export ANGZARR_PROTO_ROOT="${ANGZARR_PROTO_ROOT:-angzarr-proto}"
    cargo test --test acceptance --features acceptance-test || exit_code=$?
    kill $PF_PID 2>/dev/null || true
    exit ${exit_code:-0}

# Full acceptance test cycle: create cluster, deploy, test, cleanup
acceptance-test tag="latest": kind-create (deploy-all tag) test-e2e

# Cleanup: delete cluster
acceptance-cleanup: kind-delete

# Show cluster status
kind-status:
    #!/usr/bin/env bash
    echo "=== Cluster ==="
    kind get clusters
    echo ""
    echo "=== Pods ==="
    kubectl get pods -n angzarr-test -o wide 2>/dev/null || echo "Namespace not found"
    echo ""
    echo "=== Services ==="
    kubectl get svc -n angzarr-test 2>/dev/null || echo "Namespace not found"

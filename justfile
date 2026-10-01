# Rust examples
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

# Reusable submodule-protection recipes (install-submodule-hooks,
# check-submodules-clean). Source of truth: angzarr-project/submodule.just.
import? 'angzarr-project/submodule.just'

ROOT := `git rev-parse --show-toplevel`
IMAGE := "angzarr-examples-rust-dev"

# Build the devcontainer image
[private]
_build-image:
    docker build -t {{IMAGE}} -f "{{ROOT}}/.devcontainer/Containerfile" "{{ROOT}}/.devcontainer"

# Run just target in container (or directly if already in devcontainer)
[private]
_container +ARGS: _build-image
    #!/usr/bin/env bash
    if [ "${DEVCONTAINER:-}" = "true" ]; then
        just {{ARGS}}
    else
        # Mount the shared git dir at its host path so linked worktrees
        # (whose .git file points there) resolve inside the container.
        git_common="$(git rev-parse --path-format=absolute --git-common-dir)"
        docker run --rm \
            -v "{{ROOT}}:/workspace" \
            -v "${git_common}:${git_common}" \
            -v "{{ROOT}}/justfile.container:/workspace/justfile:ro" \
            -w /workspace \
            -e CARGO_HOME=/workspace/.cargo-container \
            -e GIT_CONFIG_COUNT=1 \
            -e GIT_CONFIG_KEY_0=safe.directory \
            -e GIT_CONFIG_VALUE_0='*' \
            {{IMAGE}} just {{ARGS}}
    fi

# Default: list available commands
[no-exit-message]
default:
    @just --list

# Build all crates (release)
build:
    just _container build

# Build all crates (debug)
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

# =============================================================================
# Kind Cluster Management (runs on host, not in container)
# =============================================================================

CLUSTER_NAME := "angzarr-test"
COORDINATOR_VERSION := "latest"

# OCI chart references
CHART_REGISTRY := "oci://ghcr.io/angzarr-io/charts"

# Ensure we use Docker Engine, not Podman socket
export DOCKER_HOST := ""

# Create kind cluster with coordinators and infrastructure
up: kind-create kind-load-coordinators deploy-infra
    @echo "=== Deployment complete ==="
    @just status

# Tear down kind cluster
down:
    kind delete cluster --name {{CLUSTER_NAME}} || true

# Show cluster status
status:
    #!/usr/bin/env bash
    echo "=== Pods ==="
    kubectl get pods -n angzarr-test -o wide 2>/dev/null || echo "Namespace not found"
    echo ""
    echo "=== Services ==="
    kubectl get svc -n angzarr-test 2>/dev/null || echo "Namespace not found"

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

# Pull and load coordinator sidecar images into kind
kind-load-coordinators:
    #!/usr/bin/env bash
    set -euo pipefail
    coordinators=(
        "angzarr-aggregate"
        "angzarr-saga"
        "angzarr-projector"
        "angzarr-grpc-gateway"
    )
    for name in "${coordinators[@]}"; do
        img="ghcr.io/angzarr-io/${name}:{{COORDINATOR_VERSION}}"
        echo "Pulling $img..."
        docker pull "$img"
        echo "Loading $img into kind..."
        kind load docker-image "$img" --name {{CLUSTER_NAME}}
    done

# Create namespace and apply base config
setup-namespace:
    #!/usr/bin/env bash
    set -euo pipefail
    kubectl create namespace angzarr-test --dry-run=client -o yaml | kubectl apply -f -

# Create image pull secret for ghcr.io (optional, for private images)
setup-pull-secret:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "${GHCR_TOKEN:-}" ]; then
        echo "GHCR_TOKEN not set, skipping pull secret (public images will still work)"
        exit 0
    fi
    kubectl create secret docker-registry ghcr-pull-secret \
        --docker-server=ghcr.io \
        --docker-username="${GHCR_USER:-$USER}" \
        --docker-password="${GHCR_TOKEN}" \
        --namespace=angzarr-test \
        --dry-run=client -o yaml | kubectl apply -f -
    kubectl patch serviceaccount default -n angzarr-test \
        -p '{"imagePullSecrets": [{"name": "ghcr-pull-secret"}]}' || true

# Deploy infrastructure (postgres, rabbitmq) via Helm
deploy-infra: setup-namespace
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Deploying PostgreSQL..."
    helm upgrade --install angzarr-db {{CHART_REGISTRY}}/angzarr-db-postgres-simple \
      --namespace angzarr-test \
      --wait --timeout 2m
    echo "Deploying RabbitMQ..."
    helm upgrade --install angzarr-mq {{CHART_REGISTRY}}/angzarr-mq-rabbitmq-simple \
      --namespace angzarr-test \
      --wait --timeout 3m
    echo "Infrastructure deployed"

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

# Auto-format code
fmt-fix:
    just _container fmt-fix

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
    docker build -t {{IMAGE}} -f "{{ROOT}}/.devcontainer/Containerfile" "{{ROOT}}/.devcontainer"

# Run just target in container (or directly if already in devcontainer)
[private]
_container +ARGS: _build-image
    #!/usr/bin/env bash
    if [ "${DEVCONTAINER:-}" = "true" ]; then
        just {{ARGS}}
    else
        docker run --rm \
            -v "{{ROOT}}:/workspace" \
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
COORDINATOR_VERSION := "latest"

# OCI chart references
CHART_REGISTRY := "oci://ghcr.io/angzarr-io/charts"
ANGZARR_CHART_VERSION := "0.5.1"

# Ensure we use Docker Engine, not Podman socket
export DOCKER_HOST := ""

# Deploy everything to kind cluster (repeatable, uses registry images)
up: kind-create kind-load-coordinators deploy-infra deploy-apps
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

# Load locally built images into kind (tags as :latest for base manifests)
kind-load-images tag="":
    #!/usr/bin/env bash
    set -euo pipefail
    images=(
        "ghcr.io/angzarr-io/examples-rust-agg-player"
        "ghcr.io/angzarr-io/examples-rust-agg-table"
        "ghcr.io/angzarr-io/examples-rust-agg-hand"
        "ghcr.io/angzarr-io/examples-rust-saga-table-hand"
        "ghcr.io/angzarr-io/examples-rust-saga-hand-player"
        "ghcr.io/angzarr-io/examples-rust-prj-output"
    )
    tag="{{tag}}"
    # If no tag specified, find the most recent skaffold-built tag
    if [ -z "$tag" ]; then
        tag=$(docker images --format '{{{{.Tag}}}}' ghcr.io/angzarr-io/examples-rust-agg-player 2>/dev/null | grep '^dev-' | head -1)
    fi
    if [ -z "$tag" ]; then
        echo "No images found. Run 'skaffold build --profile=kind' first."
        exit 1
    fi
    echo "Using tag: $tag"
    for img in "${images[@]}"; do
        src="${img}:${tag}"
        dst="${img}:latest"
        if docker image inspect "$src" &>/dev/null; then
            echo "Tagging $src as $dst..."
            docker tag "$src" "$dst"
            echo "Loading $dst into Kind..."
            kind load docker-image "$dst" --name {{CLUSTER_NAME}}
        else
            echo "Skipping $img (not found with tag $tag)"
        fi
    done

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

# Deploy poker applications using Helm
# Usage: just deploy-apps [example-tag] [coordinator-version]
# Examples:
#   just deploy-apps              # Use :latest for all
#   just deploy-apps dev-abc123   # Set example images to tag
#   just deploy-apps latest v0.1.3  # Set both example and coordinator tags
deploy-apps example_tag="latest" coordinator_version="latest":
    #!/usr/bin/env bash
    set -euo pipefail
    example_tag="{{example_tag}}"
    coord_ver="{{coordinator_version}}"

    echo "Deploying poker applications via Helm..."
    helm upgrade --install poker {{CHART_REGISTRY}}/angzarr \
      --version {{ANGZARR_CHART_VERSION}} \
      -f deploy/k8s/helm/values.yaml \
      --set images.aggregate.tag="${coord_ver}" \
      --set images.saga.tag="${coord_ver}" \
      --set images.projector.tag="${coord_ver}" \
      --set images.processManager.tag="${coord_ver}" \
      --set "applications.business[0].image.tag=${example_tag}" \
      --set "applications.business[1].image.tag=${example_tag}" \
      --set "applications.business[2].image.tag=${example_tag}" \
      --set "applications.sagas[0].image.tag=${example_tag}" \
      --set "applications.sagas[1].image.tag=${example_tag}" \
      --set "applications.projectors[0].image.tag=${example_tag}" \
      --namespace angzarr-test \
      --wait --timeout 5m

    echo "Deployment complete. Checking status:"
    kubectl get pods -n angzarr-test

# Deploy poker applications with CI overlay (imagePullSecrets)
deploy-apps-ci example_tag="latest" coordinator_version="latest":
    #!/usr/bin/env bash
    set -euo pipefail
    example_tag="{{example_tag}}"
    coord_ver="{{coordinator_version}}"

    echo "Deploying poker applications via Helm (CI mode)..."
    helm upgrade --install poker {{CHART_REGISTRY}}/angzarr \
      --version {{ANGZARR_CHART_VERSION}} \
      -f deploy/k8s/helm/values.yaml \
      -f deploy/k8s/helm/values-ci.yaml \
      --set images.aggregate.tag="${coord_ver}" \
      --set images.saga.tag="${coord_ver}" \
      --set images.projector.tag="${coord_ver}" \
      --set images.processManager.tag="${coord_ver}" \
      --set images.stream.tag="${coord_ver}" \
      --set images.gateway.tag="${coord_ver}" \
      --set images.log.tag="${coord_ver}" \
      --set "applications.business[0].image.tag=${example_tag}" \
      --set "applications.business[1].image.tag=${example_tag}" \
      --set "applications.business[2].image.tag=${example_tag}" \
      --set "applications.sagas[0].image.tag=${example_tag}" \
      --set "applications.sagas[1].image.tag=${example_tag}" \
      --set "applications.processManagers[0].image.tag=${example_tag}" \
      --set "applications.processManagers[1].image.tag=${example_tag}" \
      --set "applications.processManagers[2].image.tag=${example_tag}" \
      --set "applications.processManagers[3].image.tag=${example_tag}" \
      --set "applications.projectors[0].image.tag=${example_tag}" \
      --namespace angzarr-test \
      --wait --timeout 5m

    echo "Deployment complete. Checking status:"
    kubectl get pods -n angzarr-test

# Deploy everything to kind (uses COORDINATOR_VERSION from justfile)
deploy-all: deploy-infra
    just deploy-apps latest {{COORDINATOR_VERSION}}

# Run acceptance tests against deployed cluster
test-e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    # Wait for aggregate pods to be ready
    echo "Waiting for aggregate pods..."
    for domain in player table hand; do
        kubectl wait --for=condition=ready pod -l angzarr.io/domain=$domain \
            -n angzarr-test --timeout=180s || {
            echo "$domain aggregate pod not ready"
            kubectl get pods -n angzarr-test
            exit 1
        }
    done
    # Wait for stream pod
    echo "Waiting for stream pod..."
    kubectl wait --for=condition=ready pod -l angzarr.io/service=stream \
        -n angzarr-test --timeout=120s || {
        echo "Stream pod not ready — hand lifecycle test may fail"
        kubectl get pods -n angzarr-test
    }
    # Port-forward all aggregate coordinators + stream service
    kubectl port-forward -n angzarr-test svc/player-aggregate 1310:1310 &
    PF1=$!
    kubectl port-forward -n angzarr-test svc/table-aggregate 1311:1310 &
    PF2=$!
    kubectl port-forward -n angzarr-test svc/hand-aggregate 1312:1310 &
    PF3=$!
    kubectl port-forward -n angzarr-test svc/poker-angzarr-stream 1340:1340 &
    PF4=$!
    trap "kill $PF1 $PF2 $PF3 $PF4 2>/dev/null || true" EXIT
    # Wait for port-forwards to establish
    for port in 1310 1311 1312 1340; do
        for i in $(seq 1 10); do
            if nc -z localhost $port 2>/dev/null; then
                echo "Port-forward to localhost:$port established"
                break
            fi
            [ $i -eq 10 ] && echo "WARNING: Port $port may not be ready"
            sleep 1
        done
    done
    # Run acceptance tests
    export PLAYER_URL="http://localhost:1310"
    export TABLE_URL="http://localhost:1311"
    export HAND_URL="http://localhost:1312"
    export STREAM_URL="http://localhost:1340"
    unset ANGZARR_PROTO_ROOT
    cargo test --test acceptance --features acceptance-test

# Full local setup: build images, create cluster, deploy everything
# This mirrors what CI does, so local and CI behave identically
local-setup: kind-create
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Loading coordinator images into Kind ==="
    just kind-load-coordinators

    echo "=== Building example images with skaffold ==="
    skaffold build --profile=kind --push=false --file-output=build.json

    echo "=== Loading example images into Kind ==="
    jq -r '.builds[].tag' build.json | while read img; do
        echo "Loading $img into Kind..."
        kind load docker-image "$img" --name {{CLUSTER_NAME}}
    done

    echo "=== Deploying to Kind ==="
    just deploy-all

    echo "=== Setup complete! ==="
    just kind-status

# Full acceptance test cycle: create cluster, deploy, test, cleanup
acceptance-test: kind-create deploy-all test-e2e

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
# Trigger CI

# Auto-format code
fmt-fix:
    just _container fmt-fix

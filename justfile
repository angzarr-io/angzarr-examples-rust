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
        docker run --rm \
            -v "{{ROOT}}:/workspace" \
            -v "{{ROOT}}/justfile.container:/workspace/justfile:ro" \
            -w /workspace \
            -e CARGO_HOME=/workspace/.cargo-container \
            {{IMAGE}} just {{ARGS}}
    fi

# Run a mutation-testing target with the workspace mounted READ-ONLY.
#
# WHY:
#   cargo-mutants --in-place writes mutated source into the working tree, and
#   even with copy-mode it materialises per-mutant trees in TMPDIR. If the
#   workspace is bind-mounted RW (as `_container` does) and the container
#   dies mid-run, those mutated files can be left on the host. This helper
#   closes that hole: source is mounted at /src:ro, a tar-piped copy lands
#   in /work inside the container's WRITABLE OVERLAY LAYER, and per-mutant
#   scratch (TMPDIR) is also pinned inside /work, so `--rm` destroys
#   everything mutated on every exit.
#
# WHAT TOUCHES THE HOST:
#   - {{ROOT}}/.mutants-cache/cargo-{home,target} — compiled artifacts and
#     dep registry only. NEVER contains mutated source files. Gitignored.
#     Delete the dir to purge the cache.
#   - {{ROOT}}/mutants.out/outcomes.json — copied out at the end of a
#     successful run so external tooling can read it.
#
# WHAT NEVER TOUCHES THE HOST:
#   - Mutated source trees (live in /work, container overlay, --rm wipes).
#   - Per-mutant workspace copies (TMPDIR=/work/.scratch, also overlay).
[private]
_container-ephemeral +ARGS: _build-image
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "${DEVCONTAINER:-}" = "true" ]; then
        # Already inside a devcontainer — that container IS the ephemeral
        # boundary. Run directly; the outer just wrapper ensures --rm.
        just --justfile "{{ROOT}}/justfile.container" {{ARGS}}
        exit 0
    fi
    mkdir -p "{{ROOT}}/mutants.out" \
             "{{ROOT}}/.mutants-cache/cargo-home" \
             "{{ROOT}}/.mutants-cache/cargo-target"
    docker run --rm \
        -v "{{ROOT}}:/src:ro" \
        -v "{{ROOT}}/mutants.out:/out" \
        -v "{{ROOT}}/.mutants-cache/cargo-home:/cargo-home" \
        -v "{{ROOT}}/.mutants-cache/cargo-target:/cargo-target" \
        -v "{{ROOT}}/justfile.container:/etc/angzarr-justfile:ro" \
        -e CARGO_HOME=/cargo-home \
        -e CARGO_TARGET_DIR=/cargo-target \
        -e CARGO_MUTANTS_TMPDIR=/work/.scratch \
        -e MUTANTS_EPHEMERAL=1 \
        -e MUTANTS_OUT_DIR=/out \
        -w /work \
        {{IMAGE}} bash -eu -o pipefail -c '
            # Self-heal: install cargo-mutants on demand if the image
            # does not ship it. Cached in /cargo-home across runs.
            if ! command -v cargo-mutants >/dev/null; then
                echo "[ephemeral] cargo-mutants missing from image; installing to cached CARGO_HOME"
                cargo install cargo-mutants --locked
            fi
            echo "[ephemeral] copying /src -> /work (container overlay)"
            mkdir -p /work /work/.scratch
            # tar|tar: excludes mirror what rsync would skip — build
            # artifacts, prior mutation output, host-side cargo caches,
            # buf-exported protos (regenerated below), and the mutants
            # cache itself.
            tar -C /src \
                --exclude=./target \
                --exclude=./.cargo-container \
                --exclude=./.mutants-cache \
                --exclude=./mutants.out \
                --exclude=./mutants.out.old \
                --exclude=./mutants.out.old.2 \
                -cf - . \
                | tar -C /work -xf -
            # Mount the container-side justfile into the copy so `just` finds
            # it (the original /src is read-only, but /work is writable).
            cp /etc/angzarr-justfile /work/justfile
            cd /work
            just {{ARGS}}
            # Persist ONLY outcomes.json back to host. Mutated source trees,
            # per-mutant scratch copies, and intermediate working dirs die
            # with the container.
            if [ -f /work/mutants.out/outcomes.json ]; then
                cp /work/mutants.out/outcomes.json /out/outcomes.json
                echo "[ephemeral] outcomes.json copied to host mutants.out/"
            fi
        '

# Default: list available commands
[no-exit-message]
default:
    @just --list

# =============================================================================
# Proto generation — cross-language model (project_proto_generation_model)
# =============================================================================
# `.proto` sources live in the angzarr-project submodule. Bindings are NEVER
# committed (see .gitignore: examples-proto/, angzarr-proto/,
# proto/src/generated/*.rs). Regenerated:
#   1. on `post-checkout` / `post-merge` via lefthook
#   2. transparently as a recipe dependency of build/test/lint/check
# Idempotent: mtime guard short-circuits when bindings are newer than the
# newest .proto source.
#
# Runs in the same devcontainer image as build/test/mutation so the
# buf + protoc + tonic_prost_build toolchain is fixed. Rootless docker
# requires -u 0:0 per feedback_docker_rootless.
#
# Build-tool integration (proto/build.rs) is intentionally NOT the regen
# trigger: build.rs only runs codegen when GENERATE_PROTOS=1 is set, which
# this recipe sets. Plain `cargo build` consumes the pre-emitted
# proto/src/generated/*.rs file via `include!` in proto/src/lib.rs.

PROTO_SRC_DIR := ROOT + "/angzarr-project/proto"
PROTO_OUT_DIR := ROOT + "/proto/src/generated"

# Public entry point. Idempotent.
generate-proto:
    #!/usr/bin/env bash
    set -euo pipefail
    src_dir="{{PROTO_SRC_DIR}}"
    out_dir="{{PROTO_OUT_DIR}}"
    if [ ! -d "$src_dir" ]; then
        echo "[generate-proto] $src_dir missing — initialize angzarr-project submodule" >&2
        exit 1
    fi
    newest_proto=$(find "$src_dir" -name '*.proto' -printf '%T@\n' 2>/dev/null \
                    | sort -n | tail -1)
    if [ -d "$out_dir" ]; then
        oldest_pb=$(find "$out_dir" -name '*.rs' -printf '%T@\n' 2>/dev/null \
                        | sort -n | head -1)
    else
        oldest_pb=""
    fi
    if [ -n "$newest_proto" ] && [ -n "$oldest_pb" ] \
        && awk -v p="$newest_proto" -v b="$oldest_pb" 'BEGIN{exit !(b>p)}'; then
        echo "[generate-proto] bindings up-to-date, skipping (use generate-proto-force to override)"
        exit 0
    fi
    just generate-proto-force

# Force regeneration. Uses the devcontainer image directly because
# feedback_docker_rootless mandates `-u 0:0` for rootless writes to bind
# mounts; _container's default UID/GID only works rootful.
generate-proto-force: _build-image
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "${DEVCONTAINER:-}" = "true" ]; then
        just --justfile "{{ROOT}}/justfile.container" generate-proto-force
        exit 0
    fi
    # Detect rootless vs rootful per feedback_docker_rootless.
    if docker info --format '{{{{.SecurityOptions}}}}' 2>/dev/null | grep -q rootless; then
        USER_FLAG="-u 0:0"
    else
        USER_FLAG="-u $(id -u):$(id -g)"
    fi
    docker run --rm \
        $USER_FLAG \
        -v "{{ROOT}}:/workspace" \
        -v "{{ROOT}}/justfile.container:/workspace/justfile:ro" \
        -e CARGO_HOME=/workspace/.cargo-container \
        -e DEVCONTAINER=true \
        -w /workspace \
        {{IMAGE}} just generate-proto-force

# Build all poker aggregates (release)
build: generate-proto
    just _container build

# Build all poker aggregates (debug)
build-dev: generate-proto
    just _container build-dev

# Run unit tests (cargo --lib; mirrors Python's `test-pytest`)
test-unit: generate-proto
    just _container test-unit

# Cucumber unit-level BDD tests (mirrors Python's `test-example-unit`)
test-example-unit: generate-proto
    just _container test-example-unit

# Cucumber acceptance-level BDD tests (mirrors Python's `test-example-acceptance`)
test-example-acceptance: generate-proto
    just _container test-example-acceptance

# Back-compat alias for the pre-split BDD target
test-acceptance: generate-proto
    just _container test-acceptance

# Run all tests (unit + acceptance)
test: generate-proto
    just _container test

# Check code compiles
check: generate-proto
    just _container check

# Format code
fmt: generate-proto
    just _container fmt

# Lint code
lint: generate-proto
    just _container lint

# Clean build artifacts
clean:
    just _container clean

# Run poker in standalone mode (player:50001, table:50002, hand:50003)
run: generate-proto
    just _container run

# Run poker in standalone mode (debug build)
run-dev: generate-proto
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
deploy-apps-ci example_tag="latest" coordinator_version="":
    #!/usr/bin/env bash
    set -euo pipefail
    example_tag="{{example_tag}}"
    # Coordinator image tags come from values-ci.yaml (digest-pinned).
    # The coordinator_version arg is kept for caller compat but ignored —
    # changing it in flight would break the repo:tag lookup against what
    # was kind-loaded by the acceptance-callable pull step.

    echo "Deploying poker applications via Helm (CI mode)..."
    helm upgrade --install poker {{CHART_REGISTRY}}/angzarr \
      --version {{ANGZARR_CHART_VERSION}} \
      -f deploy/k8s/helm/values.yaml \
      -f deploy/k8s/helm/values-ci.yaml \
      --set "applications.business[0].image.tag=${example_tag}" \
      --set "applications.business[1].image.tag=${example_tag}" \
      --set "applications.business[2].image.tag=${example_tag}" \
      --set "applications.sagas[0].image.tag=${example_tag}" \
      --set "applications.sagas[1].image.tag=${example_tag}" \
      --set "applications.processManagers[0].image.tag=${example_tag}" \
      --set "applications.processManagers[1].image.tag=${example_tag}" \
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
    # Cluster is already up (kind in CI, local or external-provided) — skip
    # the in-test bootstrap-cluster.sh path; URLs below drive the clients.
    export CLUSTER_PROVIDER=external
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

# =============================================================================
# Mutation Testing
# =============================================================================
# `mutation-test` routes through `_container-ephemeral` so the mutated source
# lives in the container's writable overlay layer and is destroyed with
# `--rm`. Running cargo-mutants on the host is FORBIDDEN.
# =============================================================================

# Run mutation tests (ephemeral; no source touches host).
mutation-test: generate-proto
    just _container-ephemeral mutation-test

# Purge local mutation build cache (compiled artifacts only; no mutated source)
mutants-purge-cache:
    rm -rf "{{ROOT}}/.mutants-cache"
    @echo "Removed {{ROOT}}/.mutants-cache"

# Auto-format code
fmt-fix: generate-proto
    just _container fmt-fix

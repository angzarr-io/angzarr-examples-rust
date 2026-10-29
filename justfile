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

# Rootless docker maps container uid 0 to the host user, so `-u 0:0` is what
# makes bind-mounted writes (target/, .cargo-container/) land as the host
# user; the image's default user maps to an unowned subuid there. Rootful
# docker keeps the image default.
DOCKER_USER := if `docker info 2>/dev/null | grep -q rootless && echo yes || echo no` == "yes" { "-u 0:0" } else { "" }

# Build the devcontainer image
[private]
_build-image:
    docker build -t {{IMAGE}} -f "{{ROOT}}/.devcontainer/Containerfile" "{{ROOT}}/.devcontainer"

# Run just target in container (or directly if already in devcontainer)
[private]
_container +ARGS: _build-image
    #!/usr/bin/env bash
    if [ "${DEVCONTAINER:-}" = "true" ]; then
        # Name the container justfile explicitly. A bare `just {{ARGS}}`
        # resolves to THIS (host) justfile, whose recipes call `_container`
        # again: build -> _container build -> just build -> ... with no
        # limit. The same recursion in client-rust exhausted the cgroup's
        # task cap and killed the session running it (2026-10-26).
        just --justfile "{{ROOT}}/justfile.container" {{ARGS}}
    else
        # Mount the shared git dir at its host path so linked worktrees
        # (whose .git file points there) resolve inside the container.
        git_common="$(git rev-parse --path-format=absolute --git-common-dir)"
        docker run --rm {{DOCKER_USER}} \
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

# Architecture lint: module-level layer rules (archlint.toml)
archlint:
    just _container archlint

# Mutation-test the aggregate, the player policies and the embedded host:
# the full pass (cargo-mutants, judged per crate; thresholds in
# justfile.container)
mutation-test:
    just _container mutation-test

# Mutation-test what a push adds: lefthook's pre-push step (the full pass,
# `mutation-test`, is too slow for a hook). For each ref pushed (git's pushed
# refs on stdin), the diff from its base to the commit pushed is mutated
# (`mutation-diff`: the MUTATED crates only, judged per crate by
# MUTATION_THRESHOLDS, justfile.container), in that commit's
# tree checked out alone at target/push-tree, so an unpushed edit cannot pass
# or fail it. The base is the merge base of the commit pushed and the remote's
# current tip of that ref (a fast-forward's base is that tip); for a new ref,
# or a tip not fetched here, the merge base with the remote's default branch
# (<remote>/HEAD, else main). A deleted ref, and a ref whose diff touches no
# Rust, pass at once. Each of the push tree's submodules is the pushed
# commit's submodule commit, exported from this checkout's submodule (fetched
# if it is missing there); its .cargo-container starts as a hard-linked copy
# of this checkout's. The run's log is target/logs/mutation-push.log, its
# mutants.out target/push-tree/mutants.out.
#
# Red tests (cargo-mutants' baseline fails, exit 4) cannot be judged: pushed
# to a branch other than the default one they pass, with a warning that the
# branch cannot merge until they pass; pushed to the default branch they are
# refused. `lefthook` is what lefthook.yml passes to stop lefthook skipping
# it, and is ignored.
[positional-arguments]
mutation-push remote *lefthook:
    #!/usr/bin/env bash
    set -euo pipefail
    top='{{ROOT}}'
    remote=$1
    default=$(git -C "$top" symbolic-ref --quiet --short "refs/remotes/$remote/HEAD" 2>/dev/null) && default=${default#"$remote"/} || default=main
    tree="$top/target/push-tree"
    mkdir -p "$top/target/logs"
    log="$top/target/logs/mutation-push.log"
    exec 9> "$top/target/.push-tree.lock"
    flock 9
    pushed=0
    while read -r local_ref local_sha remote_ref remote_sha; do
        [ -n "${local_sha:-}" ] || continue
        [ "${local_sha//0/}" = "" ] && continue  # a deletion
        pushed=1
        target=${remote_ref#refs/heads/}
        base=
        if [ -n "${remote_sha//0/}" ] && git -C "$top" cat-file -e "$remote_sha^{commit}" 2>/dev/null; then
            base=$(git -C "$top" merge-base "$remote_sha" "$local_sha") || base=
        fi
        if [ -z "$base" ]; then
            for ref in "refs/remotes/$remote/$default" "refs/heads/$default"; do
                if git -C "$top" rev-parse --verify --quiet "$ref^{commit}" > /dev/null; then
                    base=$(git -C "$top" merge-base "$ref" "$local_sha") && break
                fi
            done
        fi
        if [ -z "$base" ]; then
            echo "FAIL mutation-push: $local_ref shares no history with $remote's tip or with $default" >&2
            exit 1
        fi
        range="$(git -C "$top" rev-parse --short "$base")..$(git -C "$top" rev-parse --short "$local_sha") to $remote/$target"
        if [ -z "$(git -C "$top" diff --name-only "$base" "$local_sha" -- '*.rs')" ]; then
            echo "ok   mutation-push ($range: no mutants, the push touches no Rust)"
            continue
        fi
        # The pushed commit's tree, alone; no hook of this repository runs for
        # it (core.hooksPath), and its target/ and .cargo-container/ (ignored,
        # so kept by clean) carry builds across pushes.
        nohooks=(-c core.hooksPath=/dev/null)
        if ! git -C "$tree" rev-parse --git-dir > /dev/null 2>&1; then
            rm -rf "$tree"
            git -C "$top" worktree prune
            git "${nohooks[@]}" -C "$top" worktree add --quiet --detach "$tree" "$local_sha"
        fi
        git "${nohooks[@]}" -C "$tree" checkout --quiet --detach --force "$local_sha"
        git -C "$tree" clean -fdq
        while read -r _ path; do
            sub=$(git -C "$top" ls-tree "$local_sha" "$path" | awk '{print $3}')
            [ -n "$sub" ] || continue
            if ! git -C "$top/$path" cat-file -e "$sub^{commit}" 2>/dev/null; then
                git -C "$top/$path" fetch --quiet origin "$sub" || {
                    echo "FAIL mutation-push: $path $sub, the pushed commit's submodule, is not here (git submodule update --init)" >&2
                    exit 1
                }
            fi
            rm -rf "$tree/$path"
            mkdir -p "$tree/$path"
            git -C "$top/$path" archive "$sub" | tar -x -C "$tree/$path"
        done < <(git -C "$top" config -f .gitmodules --get-regexp '^submodule\..*\.path$')
        if [ ! -d "$tree/.cargo-container" ] && [ -d "$top/.cargo-container" ]; then
            cp -al "$top/.cargo-container" "$tree/.cargo-container"
        fi
        mkdir -p "$tree/target"
        git -C "$top" diff --no-color --no-ext-diff --no-renames --src-prefix=a/ --dst-prefix=b/ \
            "$base" "$local_sha" -- '*.rs' > "$tree/target/mutants-push.patch"
        rc=0
        (cd "$tree" && just --justfile "$tree/justfile" _container mutation-diff target/mutants-push.patch) > "$log" 2>&1 9>&- || rc=$?
        out="$tree/mutants.out"
        case "$rc" in
            0) echo "ok   mutation-push ($range): $(grep -E '^(pass|No mutants in diff)' "$log" | paste -sd ';' | sed 's/;/; /g')" ;;
            4)
                if [ "$remote_ref" != "refs/heads/$default" ]; then
                    echo "RED  mutation-push: $range not judged, the tests are red; allowed off $default, which $target cannot merge into until they pass"
                else
                    echo "FAIL mutation-push: $range: red tests cannot be pushed to $remote/$target (full log: target/logs/mutation-push.log)" >&2
                    exit 1
                fi ;;
            *)
                echo "FAIL mutation-push: $range: below a crate's threshold, or the run failed (full log: target/logs/mutation-push.log)" >&2
                grep -E '^(pass|FAIL)' "$log" | sed 's/^/  /' >&2 || true
                for kind in missed; do
                    if [ -s "$out/$kind.txt" ]; then
                        echo "$kind:" >&2
                        sed 's/^/  /' "$out/$kind.txt" >&2
                    fi
                done
                [ -s "$out/missed.txt" ] && echo "Add or sharpen the test that would kill each survivor, commit it, and push again." >&2
                exit 1 ;;
        esac
    done
    [ "$pushed" = 1 ] || echo "ok   mutation-push (no commit pushed: nothing to mutate)"

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

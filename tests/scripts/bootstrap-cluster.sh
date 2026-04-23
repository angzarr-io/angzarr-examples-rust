#!/usr/bin/env bash
# bootstrap-cluster.sh — spin up / tear down the cluster backing acceptance
# tests, emitting env vars the test suite consumes.
#
# Usage:
#   bootstrap-cluster.sh <PROVIDER> <ACTION> [--dry-run] [--build]
#
#   PROVIDER : kind | external | skaffold
#   ACTION   : up | down | status
#
# On "up" prints KEY=VALUE lines on stdout; callers parse them and set env
# vars in-process. Everything else goes to stderr.
#
# External : no-op for up/down. Expects PLAYER_URL/TABLE_URL/HAND_URL/STREAM_URL
#            already set by the caller; echoes them back.
# Kind     : idempotent. Creates cluster if missing, helm-installs if release
#            is absent or not healthy, waits for pods ready, then prints URLs.
# Skaffold : reserved for future use; currently errors out.
#
# Environment knobs:
#   CLUSTER_NAME        (default: angzarr-test)
#   CLUSTER_NAMESPACE   (default: angzarr-test)
#   CLUSTER_RELEASE     (default: poker)  # Python repo overrides this
#   CLUSTER_KEEP=1      skip teardown on down
#   CLUSTER_HELM_CHART  (default: oci://ghcr.io/angzarr-io/charts/angzarr)
#   CLUSTER_HELM_VERSION (default: empty -> latest)
#   CLUSTER_VALUES      (default: deploy/k8s/helm/values.yaml)
#   CLUSTER_VALUES_CI   (default: deploy/k8s/helm/values-ci.yaml)
#   CLUSTER_KIND_CONFIG (default: deploy/kind/cluster.yaml)
#   PLAYER_URL / TABLE_URL / HAND_URL / STREAM_URL (external provider)

set -euo pipefail

PROVIDER="${1:-}"
ACTION="${2:-}"
shift 2 2>/dev/null || true

DRY_RUN=0
BUILD=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --build)   BUILD=1 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

if [ -z "$PROVIDER" ] || [ -z "$ACTION" ]; then
  echo "usage: bootstrap-cluster.sh <kind|external|skaffold> <up|down|status> [--dry-run] [--build]" >&2
  exit 2
fi

CLUSTER_NAME="${CLUSTER_NAME:-angzarr-test}"
CLUSTER_NAMESPACE="${CLUSTER_NAMESPACE:-angzarr-test}"
CLUSTER_RELEASE="${CLUSTER_RELEASE:-poker}"
CLUSTER_HELM_CHART="${CLUSTER_HELM_CHART:-oci://ghcr.io/angzarr-io/charts/angzarr}"
CLUSTER_HELM_VERSION="${CLUSTER_HELM_VERSION:-}"
CLUSTER_VALUES="${CLUSTER_VALUES:-deploy/k8s/helm/values.yaml}"
CLUSTER_VALUES_CI="${CLUSTER_VALUES_CI:-deploy/k8s/helm/values-ci.yaml}"
CLUSTER_KIND_CONFIG="${CLUSTER_KIND_CONFIG:-deploy/kind/cluster.yaml}"

# Hostport mappings defined in deploy/kind/cluster.yaml — keep in sync.
GATEWAY_HOSTPORT="${GATEWAY_HOSTPORT:-9084}"
STREAM_HOSTPORT="${STREAM_HOSTPORT:-1340}"

log()  { printf '%s\n' "[bootstrap] $*" >&2; }
run()  {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf 'DRY-RUN: %s\n' "$*" >&2
  else
    # shellcheck disable=SC2294
    eval "$@"
  fi
}

emit_urls() {
  cat <<EOF
PLAYER_URL=http://localhost:${GATEWAY_HOSTPORT}
TABLE_URL=http://localhost:${GATEWAY_HOSTPORT}
HAND_URL=http://localhost:${GATEWAY_HOSTPORT}
STREAM_URL=http://localhost:${STREAM_HOSTPORT}
EOF
}

require_tools() {
  local missing=()
  for t in "$@"; do
    command -v "$t" >/dev/null 2>&1 || missing+=("$t")
  done
  if [ ${#missing[@]} -gt 0 ]; then
    cat >&2 <<EOF
error: required tools not found: ${missing[*]}

Install them:
  kind     https://kind.sigs.k8s.io/docs/user/quick-start/#installation
  kubectl  https://kubernetes.io/docs/tasks/tools/
  helm     https://helm.sh/docs/intro/install/
  docker   https://docs.docker.com/engine/install/
EOF
    exit 3
  fi
}

# ---------------------------------------------------------------------------
# external
# ---------------------------------------------------------------------------

external_up() {
  if [ -z "${PLAYER_URL:-}" ]; then
    echo "error: CLUSTER_PROVIDER=external but PLAYER_URL is unset" >&2
    exit 4
  fi
  cat <<EOF
PLAYER_URL=${PLAYER_URL}
TABLE_URL=${TABLE_URL:-$PLAYER_URL}
HAND_URL=${HAND_URL:-$PLAYER_URL}
STREAM_URL=${STREAM_URL:-$PLAYER_URL}
EOF
}

external_down()   { :; }
external_status() {
  if [ -z "${PLAYER_URL:-}" ]; then
    echo "external provider: PLAYER_URL unset" >&2
    exit 1
  fi
  external_up
}

# ---------------------------------------------------------------------------
# kind
# ---------------------------------------------------------------------------

kind_cluster_exists() {
  kind get clusters 2>/dev/null | grep -qx "$CLUSTER_NAME"
}

helm_release_healthy() {
  # status=deployed is the only healthy terminal state
  local status
  status=$(helm status "$CLUSTER_RELEASE" -n "$CLUSTER_NAMESPACE" -o json 2>/dev/null \
    | sed -n 's/.*"status":"\([^"]*\)".*/\1/p' | head -n1)
  [ "$status" = "deployed" ]
}

pods_ready() {
  # Any pods present, and none in non-ready state
  local total
  total=$(kubectl get pods -n "$CLUSTER_NAMESPACE" --no-headers 2>/dev/null | wc -l || echo 0)
  [ "$total" -gt 0 ] || return 1
  ! kubectl get pods -n "$CLUSTER_NAMESPACE" --no-headers 2>/dev/null \
      | awk '{print $3}' | grep -vE '^(Running|Completed|Succeeded)$' | grep -q .
}

kind_up() {
  require_tools kind kubectl helm docker

  if kind_cluster_exists; then
    log "kind cluster '$CLUSTER_NAME' already exists"
  else
    log "creating kind cluster '$CLUSTER_NAME'"
    run "kind create cluster --name '$CLUSTER_NAME' --config '$CLUSTER_KIND_CONFIG'"
  fi

  run "kubectl config use-context 'kind-${CLUSTER_NAME}'"
  run "kubectl create namespace '$CLUSTER_NAMESPACE' --dry-run=client -o yaml | kubectl apply -f -"

  if [ "$BUILD" -eq 1 ]; then
    log "running 'skaffold build' (--build requested)"
    run "skaffold build --profile kind"
  fi

  if helm_release_healthy && pods_ready; then
    log "release '$CLUSTER_RELEASE' already healthy in '$CLUSTER_NAMESPACE'"
  else
    log "helm upgrade --install $CLUSTER_RELEASE"
    local version_flag=""
    [ -n "$CLUSTER_HELM_VERSION" ] && version_flag="--version '$CLUSTER_HELM_VERSION'"
    local values_flags="-f '$CLUSTER_VALUES'"
    [ -f "$CLUSTER_VALUES_CI" ] && values_flags="$values_flags -f '$CLUSTER_VALUES_CI'"
    run "helm upgrade --install '$CLUSTER_RELEASE' '$CLUSTER_HELM_CHART' $version_flag $values_flags -n '$CLUSTER_NAMESPACE' --create-namespace --wait --timeout 5m"

    log "waiting for pods ready"
    run "kubectl wait --for=condition=ready pod --all -n '$CLUSTER_NAMESPACE' --timeout=300s"
  fi

  emit_urls
}

kind_down() {
  require_tools kind kubectl helm
  if [ "${CLUSTER_KEEP:-0}" = "1" ]; then
    log "CLUSTER_KEEP=1 set, skipping teardown"
    return 0
  fi
  if helm status "$CLUSTER_RELEASE" -n "$CLUSTER_NAMESPACE" >/dev/null 2>&1; then
    run "helm uninstall '$CLUSTER_RELEASE' -n '$CLUSTER_NAMESPACE' --wait --ignore-not-found"
  fi
  if kind_cluster_exists; then
    run "kind delete cluster --name '$CLUSTER_NAME'"
  fi
}

kind_status() {
  require_tools kind kubectl helm
  if ! kind_cluster_exists; then
    echo "kind cluster '$CLUSTER_NAME' not running" >&2
    exit 1
  fi
  if ! helm_release_healthy; then
    echo "release '$CLUSTER_RELEASE' not healthy in '$CLUSTER_NAMESPACE'" >&2
    exit 1
  fi
  if ! pods_ready; then
    echo "pods in '$CLUSTER_NAMESPACE' not ready" >&2
    exit 1
  fi
  emit_urls
}

# ---------------------------------------------------------------------------
# skaffold (placeholder)
# ---------------------------------------------------------------------------

skaffold_up()     { echo "skaffold provider not yet implemented" >&2; exit 5; }
skaffold_down()   { echo "skaffold provider not yet implemented" >&2; exit 5; }
skaffold_status() { echo "skaffold provider not yet implemented" >&2; exit 5; }

# ---------------------------------------------------------------------------
# dispatch
# ---------------------------------------------------------------------------

case "$PROVIDER" in
  kind)     fn="kind_${ACTION}" ;;
  external) fn="external_${ACTION}" ;;
  skaffold) fn="skaffold_${ACTION}" ;;
  *) echo "unknown provider: $PROVIDER" >&2; exit 2 ;;
esac

case "$ACTION" in
  up|down|status) ;;
  *) echo "unknown action: $ACTION" >&2; exit 2 ;;
esac

"$fn"

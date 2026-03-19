#!/bin/bash
# Setup local development environment with Kind and local registry
# Run this script to configure Docker + Kind + registry properly

set -euo pipefail

echo "=== Local Environment Setup ==="

# 1. Stop Podman services
echo "Stopping Podman services..."
systemctl --user stop podman.socket podman.service 2>/dev/null || true
pkill -u "$(whoami)" podman 2>/dev/null || true

# 2. Ensure DOCKER_HOST is not set (use real Docker)
unset DOCKER_HOST
if grep -q "DOCKER_HOST" ~/.bashrc ~/.zshrc 2>/dev/null; then
    echo "WARNING: DOCKER_HOST is set in shell config. Please remove it."
fi

# 3. Check Docker is running
if ! docker info >/dev/null 2>&1; then
    echo "ERROR: Docker is not running. Start it with: sudo systemctl start docker"
    exit 1
fi

echo "Docker version: $(docker version --format '{{.Server.Version}}')"

# 4. Stop any existing registry on port 5001
echo "Cleaning up existing registries..."
docker stop kind-registry 2>/dev/null || true
docker rm kind-registry 2>/dev/null || true
# Kill anything on port 5001
fuser -k 5001/tcp 2>/dev/null || true

# 5. Check/create Kind cluster
echo "Checking Kind cluster..."
if ! kind get clusters 2>/dev/null | grep -q "angzarr-test"; then
    echo "Creating Kind cluster..."
    kind create cluster --name angzarr-test --config deploy/kind/cluster.yaml
else
    echo "Kind cluster 'angzarr-test' already exists"
fi

# 6. Create and connect registry
echo "Setting up local registry..."
docker run -d --restart=always --name kind-registry --network kind -p 5001:5000 registry:2

# Wait for registry
sleep 2
REG_IP=$(docker inspect kind-registry --format '{{(index .NetworkSettings.Networks "kind").IPAddress}}')
echo "Registry IP on kind network: $REG_IP"

# 7. Configure Kind to use the registry
echo "Configuring Kind containerd for registry..."
docker exec angzarr-test-control-plane mkdir -p /etc/containerd/certs.d/kind-registry:5000
docker exec angzarr-test-control-plane bash -c "cat > /etc/containerd/certs.d/kind-registry:5000/hosts.toml << EOF
server = \"http://kind-registry:5000\"
[host.\"http://kind-registry:5000\"]
  capabilities = [\"pull\", \"resolve\"]
EOF"

# Also configure for localhost:5001 (host access)
docker exec angzarr-test-control-plane mkdir -p /etc/containerd/certs.d/localhost:5001
docker exec angzarr-test-control-plane bash -c "cat > /etc/containerd/certs.d/localhost:5001/hosts.toml << EOF
server = \"http://${REG_IP}:5000\"
[host.\"http://${REG_IP}:5000\"]
  capabilities = [\"pull\", \"resolve\"]
EOF"

# 8. Set kubectl context
kubectl config use-context kind-angzarr-test

echo ""
echo "=== Setup Complete ==="
echo "Registry: localhost:5001 (host) / kind-registry:5000 (in-cluster)"
echo ""
echo "Next steps:"
echo "  1. Build coordinator images: cd ../angzarr && just build-coordinators"
echo "  2. Push to registry: docker tag <image> localhost:5001/<image>:latest && docker push localhost:5001/<image>:latest"
echo "  3. Deploy: just deploy-all"

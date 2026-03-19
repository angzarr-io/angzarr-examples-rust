#!/bin/bash
# Configure Docker to use insecure registry at localhost:30500
# Run with: sudo bash install-docker.sh

set -euo pipefail

REGISTRY="localhost:30500"

echo "Configuring Docker to use insecure registry at $REGISTRY..."

# Create daemon.json
cat > /etc/docker/daemon.json <<EOF
{
  "insecure-registries": ["$REGISTRY"]
}
EOF

echo "Restarting Docker..."
systemctl restart docker

echo "Waiting for Docker to start..."
sleep 3

echo "Docker configured. Testing registry access..."
curl -s http://$REGISTRY/v2/_catalog && echo " - Registry accessible"

echo "Done! You can now push images to $REGISTRY"

#!/bin/bash

# SSH tunnel script for HF-Lennard workflow development
# Connects to remote services on 188.245.39.229
# Stop with Ctrl-C

REMOTE_HOST="188.245.39.229"
REMOTE_USER="hf-lennard"

echo "🚀 Starting SSH tunnels to $REMOTE_USER@$REMOTE_HOST"
echo "Press Ctrl-C to stop all tunnels"
echo ""

# Create SSH tunnel with multiple port forwards
# -N: No remote command execution
# -T: Disable pseudo-terminal allocation
# -L: Local port forwarding (remote service → local)
# -R: Remote port forwarding (local service → remote)
ssh -N -T \
  -R 50051:localhost:50051 \
  -L 50052:localhost:50052 \
  -L 50053:localhost:50053 \
  -L 8000:localhost:8000 \
  $REMOTE_USER@$REMOTE_HOST \
  -o ServerAliveInterval=60 \
  -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes \
  &

SSH_PID=$!

echo "✅ SSH tunnels established (PID: $SSH_PID)"
echo ""
echo "📌 Port mappings:"
echo "  REVERSE (your local → remote can access):"
echo "    - Remote localhost:50051 → Your local workflow server :50051"
echo ""
echo "  FORWARD (remote → your local):"
echo "    - Your localhost:50052 → Remote Dossier Service :50052"
echo "    - Your localhost:50053 → Remote Letter Service :50053"
echo "    - Your localhost:8000  → Remote PDF Generator :8000"
echo ""
echo "🔧 You can now run the workflow server locally with:"
echo "  cargo run --bin workflow-server"
echo ""
echo "📱 The remote Telegram bot can reach your local server at localhost:50051"
echo ""

# Wait for SSH process and handle Ctrl-C
trap "echo ''; echo '🛑 Stopping SSH tunnels...'; kill $SSH_PID 2>/dev/null; exit 0" INT TERM

wait $SSH_PID
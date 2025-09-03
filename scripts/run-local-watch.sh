#!/bin/bash

# Development script with auto-reload on file changes
# Requires: cargo install cargo-watch

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
DATA_DIR="docker/volumes/workflow-data"
CONFIG_FILE="config/credentials.json"
GRPC_PORT="${GRPC_PORT:-50051}"

# Check if cargo-watch is installed
if ! command -v cargo-watch &> /dev/null; then
    echo -e "${YELLOW}cargo-watch is not installed${NC}"
    echo "Install it with: cargo install cargo-watch"
    echo ""
    read -p "Do you want to install it now? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        cargo install cargo-watch
    else
        exit 1
    fi
fi

# Print configuration
echo -e "${GREEN}Starting workflow server with auto-reload...${NC}"
echo -e "${YELLOW}Configuration:${NC}"
echo "  Data directory: $DATA_DIR"
echo "  Config file: $CONFIG_FILE"
echo "  gRPC port: $GRPC_PORT"
echo ""

# Ensure data directory exists
if [ ! -d "$DATA_DIR" ]; then
    echo -e "${YELLOW}Creating data directory: $DATA_DIR${NC}"
    mkdir -p "$DATA_DIR"
fi

# Check if config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${RED}Error: Config file not found: $CONFIG_FILE${NC}"
    echo "Please ensure your credentials.json is in the config/ directory"
    exit 1
fi

echo -e "${GREEN}Starting watch mode...${NC}"
echo -e "${YELLOW}The server will automatically restart when you change source files${NC}"
echo -e "${YELLOW}Press Ctrl+C to stop${NC}"
echo ""

export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

# Run with cargo-watch
cargo watch -x "run --bin workflow-server -- \
    --grpc-server \
    --grpc-port $GRPC_PORT \
    --config $CONFIG_FILE \
    --data-dir $DATA_DIR"
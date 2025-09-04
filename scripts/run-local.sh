#!/bin/bash

# Development script to run the workflow server locally with mounted data directory
# This allows rapid iteration without Docker rebuilds

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
BINARY_NAME="workflow-server"
DATA_DIR="docker/volumes/workflow-data"
LOGS_DIR="docker/volumes/logs"
TEMPLATES_DIR="templates"
CONFIG_FILE="config/credentials.json"
GRPC_PORT="${GRPC_PORT:-50051}"
BUILD_MODE="${BUILD_MODE:-release}"  # Can be 'debug' or 'release'

# Print configuration
echo -e "${GREEN}Starting local workflow server...${NC}"
echo -e "${YELLOW}Configuration:${NC}"
echo "  Binary: $BINARY_NAME"
echo "  Build mode: $BUILD_MODE"
echo "  Data directory: $DATA_DIR"
echo "  Logs directory: $LOGS_DIR"
echo "  Templates directory: $TEMPLATES_DIR"
echo "  Config file: $CONFIG_FILE"
echo "  gRPC port: $GRPC_PORT"
echo ""

# Ensure data directory exists
if [ ! -d "$DATA_DIR" ]; then
    echo -e "${RED}Data Dir not found: $DATA_DIR${NC}"   
    exit 1
fi

# Ensure logs directory exists
if [ ! -d "$LOGS_DIR" ]; then
    echo -e "${YELLOW}Creating logs directory: $LOGS_DIR${NC}"
    mkdir -p "$LOGS_DIR"
fi

# Check if templates directory exists
if [ ! -d "$TEMPLATES_DIR" ]; then
    echo -e "${RED}Error: Templates directory not found: $TEMPLATES_DIR${NC}"
    echo "Please ensure your templates directory exists with ODT template files"
    exit 1
fi

# Check if config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${RED}Error: Config file not found: $CONFIG_FILE${NC}"
    echo "Please ensure your credentials.json is in the config/ directory"
    exit 1
fi

# Build the binary
echo -e "${GREEN}Building $BINARY_NAME in $BUILD_MODE mode...${NC}"
if [ "$BUILD_MODE" = "debug" ]; then
    cargo build --bin "$BINARY_NAME"
    BINARY_PATH="target/debug/$BINARY_NAME"
else
    cargo build --release --bin "$BINARY_NAME"
    BINARY_PATH="target/release/$BINARY_NAME"
fi

if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed!${NC}"
    exit 1
fi

echo -e "${GREEN}Build successful!${NC}"
echo ""

# Run the server
echo -e "${GREEN}Starting server...${NC}"
echo -e "${YELLOW}Press Ctrl+C to stop${NC}"
echo ""

export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

exec "$BINARY_PATH" \
    --grpc-server \
    --grpc-port "$GRPC_PORT" \
    --config "$CONFIG_FILE" \
    --data-dir "$DATA_DIR" \
    --logs-dir "$LOGS_DIR" \
    --templates-dir "$TEMPLATES_DIR"
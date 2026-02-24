#!/bin/bash
# Linux Launch Script for Macropad Editor Pro

# 1. Install system dependencies (Ubuntu/Debian)
if command -v apt-get &> /dev/null
then
    echo "Installing system dependencies (libusb, pkg-config)..."
    sudo apt-get update -y && sudo apt-get install -y libusb-1.0-0-dev pkg-config build-essential
fi

# 2. Check for Rust/Cargo
if ! command -v cargo &> /dev/null
then
    echo "Rust is not installed. Installing now..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
else
    echo "Rust detected."
fi

# 3. Run the application
echo "Starting Macropad Editor Pro..."
cargo run --release -- show-gui

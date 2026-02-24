#!/bin/bash
# MacOS Launch Script for Macropad Editor Pro

# 1. Check for Rust/Cargo
if ! command -v cargo &> /dev/null
then
    echo "Rust is not installed. Installing now..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
else
    echo "Rust detected."
fi

# 2. Run the application in release mode
echo "Starting Macropad Editor Pro..."
cargo run --release -- show-gui

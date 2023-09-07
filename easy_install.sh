#!/bin/sh

# first install rust
echo "Installing Rust first..."
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# next install SciDataFlow
echo "Now installing SciDataFlow..."
cargo install scidataflow

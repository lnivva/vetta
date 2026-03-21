# Linux Installation (Ubuntu / Debian)

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Python env manager
curl -LsSf https://astral.sh/uv/install.sh | sh

# System dependencies
sudo apt-get update  
sudo apt-get install -y protobuf-compiler ffmpeg build-essential pkg-config libssl-dev
```

## Verify

```bash
rustc --version  
uv --version
protoc --version  
ffmpeg -version
```

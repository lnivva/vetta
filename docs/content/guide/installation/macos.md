# macOS Installation

## Homebrew (recommended)

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Python env manager
curl -LsSf https://astral.sh/uv/install.sh | sh

# System dependencies
brew install protobuf ffmpeg 
```

## Verify

```bash
rustc --version  
uv --version  
protoc --version  
ffmpeg -version  
```

#!/usr/bin/env bash
set -euo pipefail

echo "===== SYSTEM UPDATE ====="
sudo apt update
sudo apt upgrade -y

echo "===== BASE PACKAGES ====="
sudo apt install -y build-essential curl git ca-certificates pkg-config ffmpeg unzip htop jq protobuf-compiler

# ---------------------------------------------------------------------
# Rust (required by some deps)
# ---------------------------------------------------------------------
echo "===== INSTALL RUST ====="
if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
source "$HOME/.cargo/env"

# ---------------------------------------------------------------------
# uv (Python package manager)
# ---------------------------------------------------------------------
echo "===== INSTALL UV ====="
if ! command -v uv >/dev/null 2>&1; then
  curl -LsSf https://astral.sh/uv/install.sh | sh
fi
export PATH="$HOME/.local/bin:$PATH"

# ---------------------------------------------------------------------
# NVIDIA DRIVER (Option A with guardrails)
# ---------------------------------------------------------------------
echo "===== NVIDIA DRIVER SETUP ====="
sudo apt install -y ubuntu-drivers-common

echo "Running ubuntu-drivers autoinstall..."
sudo ubuntu-drivers autoinstall

echo "Enabling persistence mode (may fail pre-reboot)..."
sudo nvidia-smi -pm 1 || true

# ---------------------------------------------------------------------
# INSTANCE NVME SETUP
# ---------------------------------------------------------------------
echo "===== NVME SETUP ====="
NVME_DEV="/dev/nvme1n1"
NVME_MOUNT="/mnt/nvme"

if lsblk | grep -q nvme1n1; then
  sudo mkdir -p "$NVME_MOUNT"

  if ! mount | grep -q "$NVME_MOUNT"; then
    if ! sudo blkid "$NVME_DEV" >/dev/null 2>&1; then
      sudo mkfs.ext4 -F "$NVME_DEV"
    fi
    sudo mount "$NVME_DEV" "$NVME_MOUNT"
  fi

  sudo chown -R ubuntu:ubuntu "$NVME_MOUNT"
  sudo chmod 755 "$NVME_MOUNT"

  # Pre-create ALL cache directories
  mkdir -p \
    /mnt/nvme/models \
    /mnt/nvme/hf-cache \
    /mnt/nvme/torch-cache \
    /mnt/nvme/uv \
    /mnt/nvme/pip \
    /mnt/nvme/tmp
fi

# ---------------------------------------------------------------------
# ENVIRONMENT VARIABLES
# ---------------------------------------------------------------------
echo "===== ENVIRONMENT VARIABLES ====="

export HF_HOME=/mnt/nvme/hf-cache
export HF_HUB_CACHE=/mnt/nvme/hf-cache
export TRANSFORMERS_CACHE=/mnt/nvme/hf-cache
export TORCH_HOME=/mnt/nvme/torch-cache
export XDG_CACHE_HOME=/mnt/nvme
export PATH="$HOME/.local/bin:$PATH"

# Persist for future shells
cat << 'EOF' >> ~/.bashrc
export HF_HOME=/mnt/nvme/hf-cache
export HF_HUB_CACHE=/mnt/nvme/hf-cache
export TRANSFORMERS_CACHE=/mnt/nvme/hf-cache
export TORCH_HOME=/mnt/nvme/torch-cache
export XDG_CACHE_HOME=/mnt/nvme
export WHISPER_MODEL_DOWNLOAD_DIR=/mnt/nvme/models
export PATH="$HOME/.local/bin:$PATH"
EOF

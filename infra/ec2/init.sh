#!/usr/bin/env bash

# ---------------------------------------------------------------------
# Cloud-init safe bootstrap for Ubuntu 24.04 (EC2)
# ---------------------------------------------------------------------

# Log everything
exec > >(tee /var/log/vetta-init.log | logger -t vetta-init -s) 2>&1

set -euxo pipefail

echo "===== BOOTSTRAP START ====="

# ---------------------------------------------------------------------
# Fix Ubuntu EC2 mirror sync issues
# ---------------------------------------------------------------------
echo "===== FIX APT MIRRORS ====="

sed -i 's|http://.*.ec2.archive.ubuntu.com|http://archive.ubuntu.com|g' /etc/apt/sources.list

export DEBIAN_FRONTEND=noninteractive

while fuser /var/lib/dpkg/lock >/dev/null 2>&1; do
  echo "Waiting for apt lock..."
  sleep 5
done

# Retry apt update (handles transient mirror issues)
for i in {1..5}; do
  apt-get clean
  apt-get update -y && break
  echo "apt-get update failed, retrying in 10s..."
  sleep 10
done

apt-get upgrade -y

# ---------------------------------------------------------------------
# Base packages
# ---------------------------------------------------------------------
echo "===== BASE PACKAGES ====="

apt-get install -y \
  build-essential \
  curl \
  git \
  ca-certificates \
  pkg-config \
  ffmpeg \
  unzip \
  htop \
  jq \
  protobuf-compiler

# ---------------------------------------------------------------------
# Rust
# ---------------------------------------------------------------------
echo "===== INSTALL RUST (ubuntu user) ====="

sudo -u ubuntu -H bash <<'EOF'
set -e
if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
EOF

# Make Rust available system-wide
cat << 'EOF' > /etc/profile.d/rust.sh
export PATH="/home/ubuntu/.cargo/bin:$PATH"
EOF
chmod +x /etc/profile.d/rust.sh

# ---------------------------------------------------------------------
# uv
# ---------------------------------------------------------------------
echo "===== INSTALL UV ====="

sudo -u ubuntu -H bash <<'EOF'
set -e
if ! command -v uv >/dev/null 2>&1; then
  curl -LsSf https://astral.sh/uv/install.sh | sh
fi
EOF

# ---------------------------------------------------------------------
# NVIDIA DRIVER
# ---------------------------------------------------------------------
echo "===== NVIDIA DRIVER SETUP ====="

apt-get install -y ubuntu-drivers-common

echo "Running ubuntu-drivers autoinstall..."
ubuntu-drivers autoinstall || true

echo "Attempting to enable persistence mode (may fail pre-reboot)..."
nvidia-smi -pm 1 || true

# ---------------------------------------------------------------------
# INSTANCE NVME SETUP
# ---------------------------------------------------------------------
echo "===== NVME SETUP ====="

NVME_DEV="/dev/nvme1n1"
NVME_MOUNT="/mnt/nvme"

if lsblk | grep -q nvme1n1; then
  mkdir -p "$NVME_MOUNT"

  if ! mount | grep -q "$NVME_MOUNT"; then
    if ! blkid "$NVME_DEV" >/dev/null 2>&1; then
      mkfs.ext4 -F "$NVME_DEV"
    fi
    mount "$NVME_DEV" "$NVME_MOUNT"
  fi

  chown -R ubuntu:ubuntu "$NVME_MOUNT"
  chmod 755 "$NVME_MOUNT"

  mkdir -p \
    /mnt/nvme/models \
    /mnt/nvme/hf-cache \
    /mnt/nvme/torch-cache \
    /mnt/nvme/uv \
    /mnt/nvme/pip \
    /mnt/nvme/tmp

    chown -R ubuntu:ubuntu "$NVME_MOUNT"
fi

# ---------------------------------------------------------------------
# ENVIRONMENT VARIABLES
# ---------------------------------------------------------------------
echo "===== ENVIRONMENT VARIABLES ====="

cat << 'EOF' > /etc/profile.d/vetta-env.sh
export HF_HOME=/mnt/nvme/hf-cache
export HF_HUB_CACHE=/mnt/nvme/hf-cache
export TRANSFORMERS_CACHE=/mnt/nvme/hf-cache
export TORCH_HOME=/mnt/nvme/torch-cache
export XDG_CACHE_HOME=/mnt/nvme
export WHISPER_MODEL_DOWNLOAD_DIR=/mnt/nvme/models
export UV_LINK_MODE=copy
export PATH="/home/ubuntu/.local/bin:/home/ubuntu/.cargo/bin:$PATH"
EOF

chmod +x /etc/profile.d/vetta-env.sh

# ---------------------------------------------------------------------
# Finish
# ---------------------------------------------------------------------
echo "===== BOOTSTRAP COMPLETE ====="
echo "Logs available at /var/log/vetta-init.log"

# Reboot required for NVIDIA drivers
echo "===== REBOOTING FOR NVIDIA DRIVER ====="
reboot

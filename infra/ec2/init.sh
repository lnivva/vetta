#!/usr/bin/env bash

# ---------------------------------------------------------------------
# Cloud-init bootstrap for Ubuntu 24.04 (EC2)
# ---------------------------------------------------------------------

exec > >(tee /var/log/vetta-init.log | logger -t vetta-init -s) 2>&1
set -euxo pipefail

echo "===== BOOTSTRAP START ====="

export DEBIAN_FRONTEND=noninteractive
APT_OPTS=(-o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold")

# ---- Wait for any existing apt locks ----
while fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1; do
  echo "Waiting for apt lock..."
  sleep 5
done

for i in {1..5}; do
  apt-get clean
  apt-get update -y && break
  sleep 20
done

apt-get upgrade -y "${APT_OPTS[@]}"

# ---------------------------------------------------------------------
# Base packages
# ---------------------------------------------------------------------
apt-get install -y "${APT_OPTS[@]}" \
  build-essential \
  curl \
  git \
  ca-certificates \
  pkg-config \
  ffmpeg \
  unzip \
  htop \
  jq \
  protobuf-compiler \
  ubuntu-drivers-common \
  nvme-cli

# ---------------------------------------------------------------------
# Wait for ubuntu user
# ---------------------------------------------------------------------
while ! id ubuntu &>/dev/null; do
  echo "Waiting for ubuntu user..."
  sleep 2
done

# ---------------------------------------------------------------------
# Rust (ubuntu user)
# ---------------------------------------------------------------------
sudo -u ubuntu -H bash <<'EOF'
set -e
command -v rustc >/dev/null 2>&1 || \
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
EOF

cat << 'EOF' > /etc/profile.d/rust.sh
export PATH="/home/ubuntu/.cargo/bin:$PATH"
EOF
chmod +x /etc/profile.d/rust.sh

# ---------------------------------------------------------------------
# uv
# ---------------------------------------------------------------------
sudo -u ubuntu -H bash <<'EOF'
set -e
command -v uv >/dev/null 2>&1 || \
  curl -LsSf https://astral.sh/uv/install.sh | sh
EOF

# ---------------------------------------------------------------------
# NVIDIA driver
# ---------------------------------------------------------------------
ubuntu-drivers autoinstall || true

# ---------------------------------------------------------------------
# Instance NVMe setup (ephemeral)
# ---------------------------------------------------------------------
echo "===== NVME SETUP ====="

NVME_MOUNT="/mnt/nvme"

# Find instance-store NVMe (unmounted, no filesystem)
ROOT_DEV=$(findmnt -no SOURCE / | sed 's/p\?[0-9]*$//')
NVME_DEV=""
for dev in /dev/nvme*n1; do
  [ -b "$dev" ] || continue
  # Skip the root device
  [[ "$dev" == "$ROOT_DEV"* ]] && continue
  NVME_DEV="$dev"
  break
done

if [ -n "$NVME_DEV" ]; then
  echo "Found instance store: $NVME_DEV"
  mkdir -p "$NVME_MOUNT"

  mkfs.ext4 -F "$NVME_DEV"
  mount "$NVME_DEV" "$NVME_MOUNT"

  UUID=$(blkid -s UUID -o value "$NVME_DEV")
  grep -q "$UUID" /etc/fstab || \
    echo "UUID=$UUID $NVME_MOUNT ext4 defaults,nofail 0 2" >> /etc/fstab

  for d in models hf-cache torch-cache uv pip tmp; do
    mkdir -p "$NVME_MOUNT/$d"
  done

  chown -R ubuntu:ubuntu "$NVME_MOUNT"
else
  echo "WARNING: No instance-store NVMe found"
fi

# ---------------------------------------------------------------------
# Environment variables
# ---------------------------------------------------------------------
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

cat << 'EOF' > /etc/environment.d/90-vetta.conf
HF_HOME=/mnt/nvme/hf-cache
HF_HUB_CACHE=/mnt/nvme/hf-cache
TRANSFORMERS_CACHE=/mnt/nvme/hf-cache
TORCH_HOME=/mnt/nvme/torch-cache
XDG_CACHE_HOME=/mnt/nvme
WHISPER_MODEL_DOWNLOAD_DIR=/mnt/nvme/models
UV_LINK_MODE=copy
EOF

# ---------------------------------------------------------------------
# Finish
# ---------------------------------------------------------------------
echo "===== BOOTSTRAP COMPLETE ====="
echo "Logs at /var/log/vetta-init.log"

echo "===== REBOOTING FOR NVIDIA DRIVER ====="
shutdown -r +1 "Rebooting for NVIDIA driver activation"

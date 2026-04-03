#!/usr/bin/env bash
# ============================================================================
#  setup_piper_linux.sh
#
#  Downloads and installs the Piper TTS binary and required ONNX voice models
#  for generating synthetic multi-speaker test audio.
#
#  Target directory: ~/piper
# ============================================================================

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────
PIPER_DIR="$HOME/piper"
PIPER_VERSION="2023.11.14-2"
PIPER_URL="https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_x86_64.tar.gz"

# HuggingFace base URL for Piper voices
HF_BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US"

# Required voices
VOICES=(
  "amy/low/en_US-amy-low"
  "lessac/medium/en_US-lessac-medium"
  "kristin/medium/en_US-kristin-medium"
)

# ── Colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()    { echo -e "${CYAN}▸${NC} $*"; }
success() { echo -e "${GREEN}✓${NC} $*"; }
fail()    { echo -e "${RED}✗ ERROR:${NC} $*" >&2; exit 1; }

# ── Pre-flight ─────────────────────────────────────────────────────────────
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo -e "${BOLD}  Piper TTS Setup (Linux)${NC}"
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo ""

command -v curl >/dev/null 2>&1 || fail "curl is required but not installed."
command -v tar >/dev/null 2>&1 || fail "tar is required but not installed."

mkdir -p "$PIPER_DIR"
cd "$PIPER_DIR"

# ── Download Piper Binary ──────────────────────────────────────────────────
if [[ ! -f "$PIPER_DIR/piper" ]]; then
  info "Downloading Piper binary ($PIPER_VERSION)..."
  curl -L -s "$PIPER_URL" -o piper.tar.gz

  info "Extracting Piper..."
  # The tarball extracts a directory named 'piper', so we strip the first component
  # to drop the binary directly into ~/piper
  tar -xzf piper.tar.gz --strip-components=1
  rm piper.tar.gz
  success "Piper binary installed."
else
  success "Piper binary already exists in $PIPER_DIR."
fi

echo ""

# ── Download Voice Models ──────────────────────────────────────────────────
info "Downloading required voice models..."

for voice_path in "${VOICES[@]}"; do
  model_name=$(basename "$voice_path")

  # Download .onnx model
  if [[ ! -f "$PIPER_DIR/${model_name}.onnx" ]]; then
    info "Fetching ${model_name}.onnx..."
    curl -L -s "${HF_BASE}/${voice_path}.onnx" -o "${model_name}.onnx"
  else
    success "${model_name}.onnx already exists."
  fi

  # Download .onnx.json config
  if [[ ! -f "$PIPER_DIR/${model_name}.onnx.json" ]]; then
    info "Fetching ${model_name}.onnx.json..."
    curl -L -s "${HF_BASE}/${voice_path}.onnx.json" -o "${model_name}.onnx.json"
  else
    success "${model_name}.onnx.json already exists."
  fi
done

echo ""
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}  ✓ Setup Complete!${NC}"
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo -e "  Piper and voices are ready in: ${BOLD}$PIPER_DIR${NC}"
echo -e "  You can now run: ${BOLD}./_dev_script/gen_test_audio_linux.sh${NC}"
echo ""
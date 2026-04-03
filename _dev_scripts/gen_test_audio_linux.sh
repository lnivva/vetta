#!/usr/bin/env bash
# ============================================================================
#  gen_test_audio_linux.sh
#
#  Generates a synthetic multi-speaker earnings-call WAV file for testing
#  speech-to-text diarization and financial entity normalisation.
#
#  Output : /tmp/test.wav  (16 kHz, mono, PCM-16)
#  Speakers:
#    - CEO      (Amy)
#    - CFO      (Lessac)
#    - Analyst  (Kristin)
#
#  Requires:
#    - ~/piper/piper
#    - Piper ONNX voices
#    - ffmpeg
# ============================================================================

set -euo pipefail

# ── Source shared library ──────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/audio_common.sh"

# ── Platform-specific configuration ───────────────────────────────────────
PIPER_DIR="$HOME/piper"
PIPER_BIN="$PIPER_DIR/piper"

VOICE_CEO="en_US-amy-low.onnx"
VOICE_CFO="en_US-lessac-medium.onnx"
VOICE_ANALYST="en_US-kristin-medium.onnx"

# ── Generate one segment (Piper TTS) ──────────────────────────────────────
# $1 text
# $2 piper model
# $3 output wav
# $4 speaker profile
generate_segment() {
  local text="$1"
  local model="$2"
  local outfile="$3"
  local profile="$4"

  local raw="${outfile%.wav}_raw.wav"
  local norm="${outfile%.wav}_norm.wav"
  local profiled="${outfile%.wav}_profiled.wav"

  echo "$text" \
    | "$PIPER_BIN" --model "$PIPER_DIR/$model" --output_file "$raw"

  ffmpeg -nostdin -y -i "$raw" -ar "$SAMPLE_RATE" -ac 1 "$norm" 2>/dev/null
  apply_speaker_profile "$norm" "$profiled" "$profile"

  mv "$profiled" "$outfile"
  rm -f "$raw" "$norm"

  [[ -f "$outfile" ]] || fail "Failed to generate $(basename "$outfile")"
  success "$(basename "$outfile") ($(du -h "$outfile" | awk '{print $1}'))"
}

# ── Pre-flight checks ──────────────────────────────────────────────────────
print_banner "Multi-Speaker Test Audio Generator (Linux)"

info "Checking prerequisites..."

[[ -f "$PIPER_BIN" ]] || fail "Piper binary not found at $PIPER_BIN"

for model in "$VOICE_CEO" "$VOICE_CFO" "$VOICE_ANALYST"; do
  [[ -f "$PIPER_DIR/$model" ]] || fail "Missing voice model: $PIPER_DIR/$model"
done

command -v ffmpeg >/dev/null 2>&1 || fail "ffmpeg is not installed"

success "All prerequisites satisfied."
echo ""

# ── Workspace ──────────────────────────────────────────────────────────────
prepare_workspace

# ── Generate segments ──────────────────────────────────────────────────────
info "Generating speech segments..."
echo ""

info "Segment 1/4 — CEO opening (Amy)"
generate_segment \
  "good morning everyone and welcome to the mongo db q four 2024 earnings call i am the c e o and i am joined by our c f o" \
  "$VOICE_CEO" \
  "$TMP/seg1.wav" \
  ceo

info "Segment 2/4 — CFO financials (Lessac)"
generate_segment \
  "thank you on a non gaap basis e p s came in at 2 dollars and 15 cents total opex was 850 million with capex at 200 million" \
  "$VOICE_CFO" \
  "$TMP/seg2.wav" \
  cfo

info "Segment 3/4 — CEO strategy (Amy)"
generate_segment \
  "we deepened our partnership with a w s and google cloud our integration with open ai and chat gpt is driving adoption" \
  "$VOICE_CEO" \
  "$TMP/seg3.wav" \
  ceo

info "Segment 4/4 — Analyst question (Kristin)"
generate_segment \
  "hi thanks for taking my question can you talk about competitive dynamics with crowd strike and service now" \
  "$VOICE_ANALYST" \
  "$TMP/seg4.wav" \
  analyst

echo ""

# ── Concatenate & finish ───────────────────────────────────────────────────
concatenate_segments "$TMP/seg1.wav" "$TMP/seg2.wav" "$TMP/seg3.wav" "$TMP/seg4.wav"

rm -rf "$TMP"

print_summary "CEO (Amy) → CFO (Lessac) → CEO (Amy) → Analyst (Kristin)"

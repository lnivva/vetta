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
#  Speaker separability is enforced via pitch / tempo / EQ / loudness
#  fingerprints so pyannote diarization works reliably.
#
#  Requires:
#    - ~/piper/piper
#    - Piper ONNX voices
#    - ffmpeg
# ============================================================================

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────
PIPER_DIR="$HOME/piper"
PIPER_BIN="$PIPER_DIR/piper"

OUTPUT="/tmp/test.wav"
TMP="/tmp/stt_test"

VOICE_CEO="en_US-amy-low.onnx"
VOICE_CFO="en_US-lessac-medium.onnx"
VOICE_ANALYST="en_US-kristin-medium.onnx"

SAMPLE_RATE=16000
PAD_SECONDS="0.8"

# ── Colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ── Logging helpers ────────────────────────────────────────────────────────
info()    { echo -e "${CYAN}▸${NC} $*"; }
success() { echo -e "${GREEN}✓${NC} $*"; }
fail()    { echo -e "${RED}✗ ERROR:${NC} $*" >&2; exit 1; }

# ── Speaker acoustic fingerprints ──────────────────────────────────────────
# These differences are subtle to humans but large for speaker embeddings
apply_speaker_profile() {
  local in="$1"
  local out="$2"
  local profile="$3"

  case "$profile" in
    ceo)
      # Neutral, confident
      ffmpeg -nostdin -y -i "$in" \
        -af "loudnorm=I=-16:TP=-1.5:LRA=11" \
        "$out" 2>/dev/null
      ;;
    cfo)
      # Lower pitch, slower, darker
      ffmpeg -nostdin -y -i "$in" \
        -af "asetrate=16000*0.94,aresample=16000,atempo=0.96,\
             equalizer=f=180:t=q:w=1:g=-4,\
             loudnorm=I=-18:TP=-1.5:LRA=11" \
        "$out" 2>/dev/null
      ;;
    analyst)
      # Higher pitch, faster, brighter
      ffmpeg -nostdin -y -i "$in" \
        -af "asetrate=16000*1.06,aresample=16000,atempo=1.05,\
             equalizer=f=3000:t=q:w=1:g=4,\
             loudnorm=I=-14:TP=-1.5:LRA=11" \
        "$out" 2>/dev/null
      ;;
    *)
      cp "$in" "$out"
      ;;
  esac
}

# ── Generate one segment ───────────────────────────────────────────────────
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
  success "$(basename "$outfile") ($(du -h "$outfile" | cut -f1))"
}

# ── Pre-flight checks ──────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo -e "${BOLD}  Multi-Speaker Test Audio Generator${NC}"
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo ""

info "Checking prerequisites..."

[[ -f "$PIPER_BIN" ]] || fail "Piper binary not found at $PIPER_BIN"

for model in "$VOICE_CEO" "$VOICE_CFO" "$VOICE_ANALYST"; do
  [[ -f "$PIPER_DIR/$model" ]] || fail "Missing voice model: $PIPER_DIR/$model"
done

command -v ffmpeg >/dev/null 2>&1 || fail "ffmpeg is not installed"

success "All prerequisites satisfied."
echo ""

# ── Workspace ──────────────────────────────────────────────────────────────
rm -rf "$TMP"
mkdir -p "$TMP"

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

# ── Concatenate with silence ───────────────────────────────────────────────
info "Concatenating segments with ${PAD_SECONDS}s silence gaps..."

FF_FILTER=$(cat <<EOF
[0:a]apad=pad_dur=${PAD_SECONDS}[a0];
[1:a]apad=pad_dur=${PAD_SECONDS}[a1];
[2:a]apad=pad_dur=${PAD_SECONDS}[a2];
[a0][a1][a2][3:a]concat=n=4:v=0:a=1[out]
EOF
)

ffmpeg -nostdin -y \
  -i "$TMP/seg1.wav" \
  -i "$TMP/seg2.wav" \
  -i "$TMP/seg3.wav" \
  -i "$TMP/seg4.wav" \
  -filter_complex "$FF_FILTER" \
  -map "[out]" -ar "$SAMPLE_RATE" -ac 1 "$OUTPUT" 2>/dev/null

[[ -f "$OUTPUT" ]] || fail "Failed to produce $OUTPUT"

# ── Cleanup & summary ──────────────────────────────────────────────────────
rm -rf "$TMP"

DURATION=$(ffprobe -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$OUTPUT" 2>/dev/null || echo "?")
SIZE=$(du -h "$OUTPUT" | cut -f1)

echo ""
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}  ✓ Success!${NC}"
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo ""
echo -e "  File      : ${BOLD}$OUTPUT${NC}"
echo -e "  Format    : ${SAMPLE_RATE} Hz · mono · PCM‑16"
echo -e "  Duration  : ${DURATION}s"
echo -e "  Size      : ${SIZE}"
echo -e "  Speakers  : 3  (CEO → CFO → CEO → Analyst)"
echo ""

#!/usr/bin/env bash
# ============================================================================
#  audio_common.sh
#
#  Shared functions and configuration for the multi-speaker test audio
#  generators (macOS + Linux).  Sourced by the platform-specific scripts.
#
#  Usage:
#    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#    source "$SCRIPT_DIR/audio_common.sh"
# ============================================================================

# Guard against double-sourcing
[[ -n "${_AUDIO_COMMON_LOADED:-}" ]] && return 0
_AUDIO_COMMON_LOADED=1

# ── Shared configuration ──────────────────────────────────────────────────
OUTPUT="/tmp/test.wav"
TMP="/tmp/stt_test"
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
# These differences are subtle to humans but large for speaker embeddings.
# Shared across macOS and Linux generators so diarization behaviour is
# identical regardless of the TTS engine used.
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
        -af "asetrate=${SAMPLE_RATE}*0.94,aresample=${SAMPLE_RATE},atempo=0.96,\
             equalizer=f=180:t=q:w=1:g=-4,\
             loudnorm=I=-18:TP=-1.5:LRA=11" \
        "$out" 2>/dev/null
      ;;
    analyst)
      # Higher pitch, faster, brighter
      ffmpeg -nostdin -y -i "$in" \
        -af "asetrate=${SAMPLE_RATE}*1.06,aresample=${SAMPLE_RATE},atempo=1.05,\
             equalizer=f=3000:t=q:w=1:g=4,\
             loudnorm=I=-14:TP=-1.5:LRA=11" \
        "$out" 2>/dev/null
      ;;
    *)
      cp "$in" "$out"
      ;;
  esac
}

# ── Concatenate segments with silence gaps ─────────────────────────────────
# Accepts a variable number of segment paths.  Inserts PAD_SECONDS of
# silence after every segment except the last, then writes OUTPUT.
concatenate_segments() {
  local segments=("$@")
  local n=${#segments[@]}

  info "Concatenating ${n} segments with ${PAD_SECONDS}s silence gaps..."

  # Build ffmpeg inputs
  local inputs=()
  for seg in "${segments[@]}"; do
    inputs+=(-i "$seg")
  done

  # Build filter graph
  local filter=""
  local concat_inputs=""
  for (( i=0; i<n; i++ )); do
    if (( i < n - 1 )); then
      filter+="[${i}:a]apad=pad_dur=${PAD_SECONDS}[a${i}];"$'\n'
      concat_inputs+="[a${i}]"
    else
      concat_inputs+="[${i}:a]"
    fi
  done
  filter+="${concat_inputs}concat=n=${n}:v=0:a=1[out]"

  ffmpeg -nostdin -y \
    "${inputs[@]}" \
    -filter_complex "$filter" \
    -map "[out]" -ar "$SAMPLE_RATE" -ac 1 "$OUTPUT" 2>/dev/null

  [[ -f "$OUTPUT" ]] || fail "Failed to produce $OUTPUT"
}

# ── Print final summary ────────────────────────────────────────────────────
# $1  speaker description string, e.g. "CEO (Amy) → CFO (Lessac) → …"
print_summary() {
  local speakers="$1"

  local duration
  duration=$(ffprobe -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$OUTPUT" 2>/dev/null || echo "?")
  local size
  size=$(du -h "$OUTPUT" | awk '{print $1}')

  echo ""
  echo -e "${BOLD}═══════════════════════════════════════════${NC}"
  echo -e "${GREEN}${BOLD}  ✓ Success!${NC}"
  echo -e "${BOLD}═══════════════════════════════════════════${NC}"
  echo ""
  echo -e "  File      : ${BOLD}$OUTPUT${NC}"
  echo -e "  Format    : ${SAMPLE_RATE} Hz · mono · PCM‑16"
  echo -e "  Duration  : ${duration}s"
  echo -e "  Size      : ${size}"
  echo -e "  Speakers  : ${speakers}"
  echo ""
}

# ── Prepare workspace ─────────────────────────────────────────────────────
prepare_workspace() {
  rm -rf "$TMP"
  mkdir -p "$TMP"
}

# ── Print banner ──────────────────────────────────────────────────────────
# $1  title string
print_banner() {
  local title="$1"
  echo ""
  echo -e "${BOLD}═══════════════════════════════════════════${NC}"
  echo -e "${BOLD}  ${title}${NC}"
  echo -e "${BOLD}═══════════════════════════════════════════${NC}"
  echo ""
}

#!/usr/bin/env bash
# ============================================================================
#  gen_test_audio_macos.sh
#
#  Generates a synthetic multi-speaker earnings-call WAV file for testing
#  speech-to-text diarization and financial entity normalisation.
#
#  Output : /tmp/test.wav  (16 kHz, mono, PCM-16)
#  Speakers:
#    - CEO      (Samantha)
#    - CFO      (Fred)
#    - Analyst  (Melina)
#
#  Requires:
#    - macOS built-in 'say' command
#    - ffmpeg (e.g., via Homebrew)
# ============================================================================

set -euo pipefail

# ── Source shared library ──────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/audio_common.sh"

# ── Platform-specific configuration ───────────────────────────────────────
# Standard macOS voices. You can check available voices with: say -v '?'
VOICE_CEO="Samantha"
VOICE_CFO="Fred"
VOICE_ANALYST="Melina"

# ── Generate one segment (macOS 'say') ─────────────────────────────────────
# $1 text
# $2 macOS voice
# $3 output wav
# $4 speaker profile
generate_segment() {
  local text="$1"
  local voice="$2"
  local outfile="$3"
  local profile="$4"

  local raw_aiff="${outfile%.wav}_raw.aiff"
  local norm="${outfile%.wav}_norm.wav"
  local profiled="${outfile%.wav}_profiled.wav"

  # Generate audio using macOS 'say' (outputs AIFF format by default)
  say -v "$voice" "$text" -o "$raw_aiff"

  # Convert AIFF to 16kHz mono WAV
  ffmpeg -nostdin -y -i "$raw_aiff" -ar "$SAMPLE_RATE" -ac 1 "$norm" 2>/dev/null

  # Apply the acoustic fingerprint
  apply_speaker_profile "$norm" "$profiled" "$profile"

  mv "$profiled" "$outfile"
  rm -f "$raw_aiff" "$norm"

  [[ -f "$outfile" ]] || fail "Failed to generate $(basename "$outfile")"
  success "$(basename "$outfile") ($(du -h "$outfile" | awk '{print $1}'))"
}

# ── Pre-flight checks ──────────────────────────────────────────────────────
print_banner "Multi-Speaker Test Audio Generator (macOS)"

info "Checking prerequisites..."

command -v say >/dev/null 2>&1 || fail "'say' command not found. Are you on macOS?"
command -v ffmpeg >/dev/null 2>&1 || fail "ffmpeg is not installed. Run 'brew install ffmpeg'."

# Verify voices exist
for voice in "$VOICE_CEO" "$VOICE_CFO" "$VOICE_ANALYST"; do
  say -v '?' | grep -q "^$voice " || fail "Voice '$voice' not found. Run 'say -v ?' to see available voices."
done

success "All prerequisites satisfied."
echo ""

# ── Workspace ──────────────────────────────────────────────────────────────
prepare_workspace

# ── Generate segments ──────────────────────────────────────────────────────
info "Generating speech segments..."
echo ""

info "Segment 1/4 — CEO opening ($VOICE_CEO)"
generate_segment \
  "good morning everyone and welcome to the mongo db q four 2024 earnings call i am the c e o and i am joined by our c f o" \
  "$VOICE_CEO" \
  "$TMP/seg1.wav" \
  ceo

info "Segment 2/4 — CFO financials ($VOICE_CFO)"
generate_segment \
  "thank you on a non gaap basis e p s came in at 2 dollars and 15 cents total opex was 850 million with capex at 200 million" \
  "$VOICE_CFO" \
  "$TMP/seg2.wav" \
  cfo

info "Segment 3/4 — CEO strategy ($VOICE_CEO)"
generate_segment \
  "we deepened our partnership with a w s and google cloud our integration with open ai and chat gpt is driving adoption" \
  "$VOICE_CEO" \
  "$TMP/seg3.wav" \
  ceo

info "Segment 4/4 — Analyst question ($VOICE_ANALYST)"
generate_segment \
  "hi thanks for taking my question can you talk about competitive dynamics with crowd strike and service now" \
  "$VOICE_ANALYST" \
  "$TMP/seg4.wav" \
  analyst

echo ""

# ── Concatenate & finish ───────────────────────────────────────────────────
concatenate_segments "$TMP/seg1.wav" "$TMP/seg2.wav" "$TMP/seg3.wav" "$TMP/seg4.wav"

rm -rf "$TMP"

print_summary "CEO ($VOICE_CEO) → CFO ($VOICE_CFO) → CEO ($VOICE_CEO) → Analyst ($VOICE_ANALYST)"

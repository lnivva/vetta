#!/bin/bash
set -e

# ============================================================================
# Test audio generator for post-processing validation
#
# Covers: entity correction, acronym normalization, financial terms,
#         quarter names, title expansion, punctuation, truecasing,
#         multi-speaker diarization, and segment stitching.
# ============================================================================

OUTPUT="/tmp/test.wav"
TMP="/tmp/stt_test"
mkdir -p "$TMP"

# Speaker 1: CEO opening — tests company names, quarters, titles, truecasing
say -v Samantha "good morning everyone and welcome to the mongo db q four 2024 earnings call i am the c e o and i am joined by our c f o we are pleased to report record a r r of 1.8 billion dollars this represents 30 percent year over year growth" -o "$TMP/seg1.aiff"

# Speaker 2: CFO financials — tests GAAP/non-GAAP, OpEx, CapEx, EPS, EBITDA
say -v Daniel "thank you on a non gaap basis e p s came in at 2 dollars and 15 cents total opex was 850 million with capex at 200 million ebitda margin expanded to 35 percent our saas revenue now represents 78 percent of total revenue" -o "$TMP/seg2.aiff"

# Speaker 1: Strategy — tests cloud providers, product names, AI terms
say -v Samantha "let me provide some color on our strategy we deepened our partnership with a w s and google cloud our integration with open ai and chat gpt is driving strong adoption we also launched new features on data bricks and snow flake" -o "$TMP/seg3.aiff"

# Speaker 2: Guidance — tests quarter refs, financial metrics, hyphenated terms
say -v Daniel "looking ahead to q one 2025 we expect revenue between 1.1 and 1.15 billion this implies quarter over quarter growth of approximately 8 percent we are raising our full year f c f guidance to 600 million our r o i on cloud infrastructure continues to improve" -o "$TMP/seg4.aiff"

# Speaker 1: Q&A transition — tests sentence stitching across short segments
say -v Samantha "thank you for that overview let me now open the floor for questions from our analysts" -o "$TMP/seg5.aiff"

# Speaker 3: Analyst question — tests natural speech patterns, company names
say -v Karen "hi thanks for taking my question can you talk about competitive dynamics with crowd strike and service now and how you see your t a m expanding with the data dog partnership also any update on the i p o pipeline for your venture investments" -o "$TMP/seg6.aiff"

# Speaker 1: Answer — tests entity density, mixed acronyms, CapEx/OpEx together
say -v Samantha "great question our sales force integration with hub spot went live in q three we see significant expansion in our p a a s and i a a s offerings the cagr for our cloud business is trending above 40 percent we remain confident in our competitive positioning versus cloud flair and palo alto" -o "$TMP/seg7.aiff"

# Combine all segments with short silence gaps between speakers
ffmpeg -y \
  -i "$TMP/seg1.aiff" \
  -i "$TMP/seg2.aiff" \
  -i "$TMP/seg3.aiff" \
  -i "$TMP/seg4.aiff" \
  -i "$TMP/seg5.aiff" \
  -i "$TMP/seg6.aiff" \
  -i "$TMP/seg7.aiff" \
  -filter_complex "\
    [0:a]apad=pad_dur=0.8[a0]; \
    [1:a]apad=pad_dur=0.8[a1]; \
    [2:a]apad=pad_dur=0.8[a2]; \
    [3:a]apad=pad_dur=0.8[a3]; \
    [4:a]apad=pad_dur=0.5[a4]; \
    [5:a]apad=pad_dur=0.8[a5]; \
    [a0][a1][a2][a3][a4][a5][6:a]concat=n=7:v=0:a=1[out]" \
  -map "[out]" \
  -ar 16000 -ac 1 \
  "$OUTPUT"

# Cleanup temp files
rm -rf "$TMP"

echo ""
echo "============================================"
echo "Test audio written to: $OUTPUT"
echo "============================================"
echo ""
echo "Expected post-processing corrections:"
echo "  mongo db        → MongoDB"
echo "  q four          → Q4"
echo "  q one           → Q1"
echo "  q three         → Q3"
echo "  c e o           → CEO"
echo "  c f o           → CFO"
echo "  a r r           → ARR"
echo "  non gaap        → non-GAAP"
echo "  e p s           → EPS"
echo "  opex            → OpEx"
echo "  capex           → CapEx"
echo "  ebitda          → EBITDA"
echo "  saas            → SaaS"
echo "  a w s           → AWS"
echo "  google cloud    → Google Cloud"
echo "  open ai         → OpenAI"
echo "  chat gpt        → ChatGPT"
echo "  data bricks     → Databricks"
echo "  snow flake      → Snowflake"
echo "  crowd strike    → CrowdStrike"
echo "  service now     → ServiceNow"
echo "  data dog        → Datadog"
echo "  hub spot        → HubSpot"
echo "  cloud flair     → Cloudflare"
echo "  palo alto       → Palo Alto"
echo "  sales force     → Salesforce"
echo "  t a m           → TAM"
echo "  r o i           → ROI"
echo "  f c f           → FCF"
echo "  i p o           → IPO"
echo "  p a a s         → PaaS"
echo "  i a a s         → IaaS"
echo "  cagr            → CAGR"
echo "  year over year  → year-over-year"
echo "  quarter over quarter → quarter-over-quarter"
echo ""
echo "Expected diarization: 3 speakers"
echo "  Speaker A (Samantha): seg1, seg3, seg5, seg7"
echo "  Speaker B (Daniel):   seg2, seg4"
echo "  Speaker C (Karen):    seg6"
echo ""
echo "Run with:"
echo "  cargo run -- earnings process --file $OUTPUT --ticker TEST --year 2024 --quarter q4 --print --replace"

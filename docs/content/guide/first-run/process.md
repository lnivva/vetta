# Processing Audio

Once you have generated your test audio at `/tmp/test.wav`, you can run the full transcription, diarization, and
post-processing pipeline using the `vetta` CLI.

## 1. Run the Pipeline

From the project root (where `Cargo.toml` lives):

```bash  
cargo run -- earnings process --file /tmp/test.wav --ticker TEST --year 2024 --quarter q4 --print --replace  
```  

## 2. Parameter Breakdown

| Flag                   | Description                                                                          |  
|:-----------------------|:-------------------------------------------------------------------------------------|  
| `earnings process`     | Subcommand for handling earnings call audio.                                         |  
| `--file /tmp/test.wav` | Path to the audio file generated in the [previous step](/guide/generate-test-audio). |  
| `--ticker TEST`        | Company identifier. Used for metadata and storage naming.                            |  
| `--year 2024`          | Fiscal year associated with the call.                                                |  
| `--quarter q4`         | Fiscal quarter associated with the call.                                             |  
| `--print`              | **Debug mode.** Streams the transcript to your terminal in real time.                |  c
| `--replace`            | Overwrites any existing transcript for this Ticker / Year / Quarter combination.     |  

## 3. What Happens Behind the Scenes

The command triggers a multi-stage pipeline:

```text  
┌─────────────┐     ┌──────────────────┐     ┌──────────────────┐     ┌────────────────┐  
│  Audio      │     │  Transcription   │     │  Post-Processing │     │  Output        │  
│  Ingestion  │────▸│  + Diarization   │────▸│  + Normalization │────▸│  (print/store) │  
└─────────────┘     └──────────────────┘     └──────────────────┘     └────────────────┘  
```  

| Stage                         | What it does                                                                 |  
|:------------------------------|:-----------------------------------------------------------------------------|  
| **Audio Ingestion**           | The CLI sends the WAV file to the gRPC service.                              |  
| **Transcription** _(Whisper)_ | Converts speech to raw text segments with timestamps.                        |  
| **Diarization** _(Pyannote)_  | Runs in parallel — identifies distinct speakers by voice characteristics.    |  
| **Segment Stitching**         | Merges short segments from the same speaker into readable paragraphs.        |  
| **Entity Correction**         | `mongo db` → **MongoDB**, `a w s` → **AWS**, `non gaap` → **non-GAAP**, etc. |  
| **Truecasing**                | Proper nouns and sentence starts are correctly capitalized.                  |  

## 4. Expected Output

With `--print` enabled, the terminal output should look similar to this:

```text  
[00:00:00 -> 00:00:12] [Speaker 0]  
Good morning everyone and welcome to the MongoDB Q4 2024 earnings call. I am the CEO and I am joined by our CFO.
  
[00:00:13 -> 00:00:25] [Speaker 1]  
Thank you. On a non-GAAP basis, EPS came in at $2.15. Total OpEx was $850 million with CapEx at $200 million.
  
[00:00:26 -> 00:00:36] [Speaker 0]  
We deepened our partnership with AWS and Google Cloud. Our integration with OpenAI and ChatGPT is driving adoption.
  
[00:00:37 -> 00:00:44] [Speaker 2]  
Hi, thanks for taking my question. Can you talk about competitive dynamics with CrowdStrike and ServiceNow?
```  

### Verification Checklist

Use this table to confirm each pipeline stage is working correctly:

| Check               | What to look for                                                           |  
|:--------------------|:---------------------------------------------------------------------------|  
| **Speaker IDs**     | Speaker ID changes when the voice changes (`0 → 1 → 0 → 2`).               |  
| **Financial terms** | `non-GAAP`, `EPS`, `OpEx`, `CapEx` are capitalised correctly.              |  
| **Numbers**         | "two dollars and fifteen cents" → `$2.15`, "850 million" → `$850 million`. |  
| **Company names**   | `MongoDB`, `AWS`, `OpenAI`, `ChatGPT`, `CrowdStrike`, `ServiceNow`.        |  
| **Speaker reuse**   | Speaker 0 appears in segments 1 _and_ 3 (same voice = same ID).            |  

## 5. Troubleshooting

:::warning Connection Refused

The CLI connects to the STT gRPC service, which must be running first. Start it from the service directory:

```bash  
cd services/stt/local/  
uv run python main.py --config config.toml  
```  

If this is your first time setting up the service, follow the full [STT Service setup guide](/guide/services/stt) to
install dependencies and configure `config.toml`.  
:::

:::warning Model Download on First Run  

The first execution downloads several GB of model weights (Whisper + Pyannote). Watch the **server logs** for download
progress — the CLI may appear to hang until the models are cached locally.  

:::

:::warning Diarization Returns a Single Speaker  

This usually means the Hugging Face auth token is missing or invalid. Pyannote requires a token with access to gated
models.

1. Ensure `diarization.enabled = true` and `diarization.hf_token` is set in your `config.toml`
2. Verify you have accepted the model licenses on Hugging Face

See the [STT Service guide](/guide/services/stt#3-configure-hugging-face-access) and
the [Configuration Reference](/configuration/stt-service#hf_token-setup) for details.  

:::
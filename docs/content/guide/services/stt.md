# STT Service

The STT (Speech-to-Text) service is the primary audio processing engine of Vetta, responsible for converting raw audio
recordings into analysis-ready, time-aligned transcripts.

## Local Setup

The local STT service is designed for performance and data confidentiality. It is typically executed within its specific
service directory.

### 1. Install Dependencies

Navigate to the STT local service directory and use the provided `Makefile` to set up the Python environment and
dependencies:

```bash
cd services/stt/local/
make setup
```

### 2. Configuration (`config.toml`)

The service requires a `config.toml` file in the service directory to define model parameters, hardware usage, and
diarization settings.

Create or update your `config.toml` with the following recommended structure:

```toml
[service]
address = "unix:///tmp/whisper.sock" # or "0.0.0.0:50051" for TCP
log_level = "info"
max_audio_size_mb = 100

[model]
size = "large-v3"
download_dir = "/tmp/whisper_models"
device = "cpu" # Change to "cuda" if using a GPU
compute_type = "int8"

[inference]
beam_size = 5
vad_filter = true
vad_min_silence_ms = 300
no_speech_threshold = 0.6
log_prob_threshold = -0.5
compression_ratio_threshold = 2.0
word_timestamps = true
initial_prompt = ""

[concurrency]
max_workers = 1
cpu_threads = 8
num_workers = 1

[diarization]
enabled = true
hf_token = "YOUR_HUGGING_FACE_TOKEN" # Required for speaker attribution
model = "pyannote/speaker-diarization-3.1"
device = "cpu" # Align with model.device
min_speakers = 0
max_speakers = 0

[postprocessing]
enabled = true
punctuation = true
entity_correction = true
truecasing = true
```

For a detailed breakdown of every configuration property, see the [Configuration Reference](/configuration/stt-service)

### 3. Configure Hugging Face Access

Since the service relies on protected models for diarization, you must ensure your Hugging Face authentication is
configured correctly and the token is added to your config.

## Running the Service

The STT service is executed using `uv` to manage the Python environment.

To start the service, run:

```bash
uv run python main.py --config config.toml
```

## Key Features

* **Transcription:** Produces a clean, searchable transcript aligned to the audio recording.
* **Speaker Diarization:** Attributes statements to the correct participants, such as distinguishing between company
  management and analysts.
* **Format Support:** While the service can process various formats via `ffmpeg` internal conversion, using **WAV (16kHz
  mono)** is recommended to avoid additional processing time.
* **Local Processing:** Ensures data confidentiality and reproducibility by running inference locally rather than
  through a cloud API.

::: tip Performance
For significantly faster Whisper inference, it is recommended to update the `device` settings in `config.toml` to `cuda`
and run this service on a machine with an NVIDIA GPU.
:::
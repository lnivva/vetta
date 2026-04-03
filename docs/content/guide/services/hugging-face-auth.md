# Hugging Face Authentication Guide

This service uses **Pyannote Audio** for speaker diarization. Because these models are "gated," you must authenticate
with Hugging Face to download and use them.

## 1. Why is a token required?

The default diarization model (`pyannote/speaker-diarization-3.1`) is hosted on the Hugging Face Hub under a custom
license. To use it, you must:

1. Have a Hugging Face account.
2. Accept the user conditions on the model page.
3. Provide an Access Token so the service can verify your permissions.

## 2. Setup Instructions

### Step A: Accept Model Terms

You must manually visit these two pages while logged into Hugging Face and click **"Accept"** on the agreement banners:

* [pyannote/speaker-diarization-3.1](https://huggingface.co/pyannote/speaker-diarization-3.1)
* [pyannote/segmentation-3.0](https://huggingface.co/pyannote/segmentation-3.0)

### Step B: Create an Access Token

1. Go to [huggingface.co/settings/tokens](https://huggingface.co/settings/tokens).
2. Click **"New token"**.
3. Set the Type to **"Read"**.
4. Name it something descriptive (e.g., `Whisper-STT-Service`).
5. Copy the token (it starts with `hf_...`).

## 3. Configuration

The service is designed to read the token directly from your `config.toml`. This avoids the need to manage system-level
environment variables.

Open your `config.toml` and update the `[diarization]` section:

```toml
[diarization]
enabled = true
hf_token = "hf_YOUR_TOKEN_HERE"  # Paste your token here
model = "pyannote/speaker-diarization-3.1"
required = true
```

## 4. How it works in the Code

When the `WhisperServicer` initializes, it checks if `hf_token` is populated in the settings. It then performs a
programmatic login:

```python
# From servicer.py
if s.diarization.enabled and s.diarization.hf_token:
    from huggingface_hub import login

    login(token=s.diarization.hf_token)
```

This caches the credentials in `~/.cache/huggingface/`, allowing the diarization pipeline to download the necessary
weights securely.

## 5. Troubleshooting

* **403 Forbidden:** This usually means you haven't accepted the terms for *both* the diarization and segmentation
  models mentioned in Step A.
* **Unauthenticated Warning:** If you see `Warning: You are sending unauthenticated requests`, check that your
  `config.toml` path is correct and the `hf_token` key is not empty.
* **Rate Limiting:** Without a token, Hugging Face severely limits download speeds and frequency, which may cause the
  service to hang during the first initialization.

### Security Note

**Never commit your `config.toml` to version control (Git) if it contains a real token.** Use a template file or ensure
`config.toml` is added to your `.gitignore`.
import textwrap
from pathlib import Path
from unittest.mock import MagicMock

import pytest


# ── Shared fixtures ────────────────────────────────────────────────────────────

@pytest.fixture
def minimal_config(tmp_path: Path) -> Path:
    """
    Create a minimal valid config.toml in the provided temporary directory.
    
    The file contains service, model, inference, and concurrency sections with basic settings used by tests.
    
    Parameters:
        tmp_path (Path): Temporary directory in which to create the config.toml file.
    
    Returns:
        Path: Path to the created config.toml.
    """
    cfg = tmp_path / "config.toml"
    cfg.write_text(textwrap.dedent("""\
        [service]
        socket_path = "/tmp/test_whisper.sock"
        log_level   = "info"

        [model]
        size         = "small"
        download_dir = "/tmp/whisper_models"
        device       = "cpu"
        compute_type = "int8"

        [inference]
        beam_size                   = 3
        vad_filter                  = true
        vad_min_silence_ms          = 500
        no_speech_threshold         = 0.6
        log_prob_threshold          = -1.0
        compression_ratio_threshold = 2.4
        word_timestamps             = true
        initial_prompt              = ""

        [concurrency]
        max_workers = 1
        cpu_threads = 2
        num_workers = 1
    """))
    return cfg


@pytest.fixture(scope="module")
def mock_whisper_model():
    """
    Create a MagicMock simulating a WhisperModel with a predefined transcription result.
    
    The mock's transcribe.return_value is a tuple of (segments, info) where segments is a list containing a single fake segment (with text "  Hello world  ", start/end times, avg_logprob and a single fake word) and info contains language ("en") and language_probability (0.98). Attach or replace model.transcribe.return_value in tests to customize behavior.
    
    Returns:
        MagicMock: A mock WhisperModel whose transcribe method returns (segments, info) by default.
    """
    fake_word = MagicMock()
    fake_word.start = 0.0
    fake_word.end = 0.5
    fake_word.word = "Hello"
    fake_word.probability = 0.99

    fake_segment = MagicMock()
    fake_segment.start = 0.0
    fake_segment.end = 3.5
    fake_segment.text = "  Hello world  "
    fake_segment.avg_logprob = -0.3
    fake_segment.words = [fake_word]

    fake_info = MagicMock()
    fake_info.language = "en"
    fake_info.language_probability = 0.98

    model = MagicMock()
    model.transcribe.return_value = ([fake_segment], fake_info)
    return model
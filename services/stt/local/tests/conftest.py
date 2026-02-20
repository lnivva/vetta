import textwrap
from pathlib import Path
from unittest.mock import MagicMock

import pytest


# ── Shared fixtures ────────────────────────────────────────────────────────────


@pytest.fixture
def minimal_config(tmp_path: Path) -> Path:
    """
    Create a minimal valid config.toml in a temporary directory.

    Writes a TOML configuration containing [service], [model], [inference], and [concurrency]
    sections suitable for tests, and returns the path to the created config file.

    Parameters:
        tmp_path (Path): Temporary directory path (pytest tmp_path fixture) where the file will be written.

    Returns:
        Path: Path to the written config.toml file.
    """
    cfg = tmp_path / "config.toml"
    cfg.write_text(
        textwrap.dedent("""\
        [service]
        socket_path = "/tmp/test_whisper.sock"
        log_level   = "info"
        max_audio_size_mb = 100

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
    """)
    )
    return cfg


@pytest.fixture(scope="module")
def mock_whisper_model():
    """
    Create a MagicMock that simulates a WhisperModel returning one transcription segment and language metadata.

    The mock's transcribe method returns a two-tuple: (segments, info). `segments` is a list with one object that exposes attributes:
    - start (float), end (float)
    - text (str) with surrounding whitespace
    - avg_logprob (float)
    - words (list) containing one word object with attributes: start (float), end (float), word (str), probability (float)

    `info` is an object with attributes:
    - language (str)
    - language_probability (float)

    Returns:
        MagicMock: A mock model whose `transcribe` method returns ([fake_segment], fake_info).
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

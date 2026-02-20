import io
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import grpc
import pytest

from servicer import WhisperServicer
from settings import (
    Settings,
    ServiceConfig,
    ModelConfig,
    InferenceConfig,
    ConcurrencyConfig,
)
from speech import speech_pb2


def make_settings(tmp_dir: Path, **inference_overrides) -> Settings:
    """Builds a Settings object with sane defaults."""
    inference_defaults: dict[str, Any] = dict(
        beam_size=5,
        vad_filter=True,
        vad_min_silence_ms=500,
        no_speech_threshold=0.6,
        log_prob_threshold=-1.0,
        compression_ratio_threshold=2.4,
        word_timestamps=True,
        initial_prompt="",
    )
    inference_defaults.update(inference_overrides)
    return Settings(
        service=ServiceConfig(
            socket_path=str(tmp_dir / "t.sock"), log_level="info", max_audio_size_mb=10
        ),
        model=ModelConfig(
            size="small", download_dir=str(tmp_dir), device="cpu", compute_type="int8"
        ),
        inference=InferenceConfig(**inference_defaults),
        concurrency=ConcurrencyConfig(max_workers=1, cpu_threads=2, num_workers=1),
    )


@pytest.fixture
def servicer(mock_whisper_model, tmp_path):
    """Create a WhisperServicer with its WhisperModel patched."""
    with patch("servicer.WhisperModel", return_value=mock_whisper_model):
        svc = WhisperServicer(make_settings(tmp_path))
    svc.model = mock_whisper_model
    return svc


def make_request(path="test.mp3", language="en", initial_prompt=""):
    """
    Create a mocked gRPC request object configured to represent a 'path' payload.
    Synchronized with speech.proto 'oneof audio_source'.
    """
    options = MagicMock()
    options.initial_prompt = initial_prompt
    req = MagicMock()

    # The oneof field name in .proto is 'path'
    req.WhichOneof.return_value = "path"
    req.path = path

    req.language = language
    req.options = options
    return req


class TestWhisperServicer:
    def test_yields_transcript_chunks(self, servicer):
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert len(chunks) == 1
        assert isinstance(chunks[0], speech_pb2.TranscriptChunk)

    def test_text_is_stripped(self, servicer):
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert chunks[0].text == "Hello world"

    def test_timing_fields(self, servicer):
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert chunks[0].start_time == pytest.approx(0.0)
        assert chunks[0].end_time == pytest.approx(3.5)

    def test_request_initial_prompt_takes_priority(self, servicer, mock_whisper_model):
        list(
            servicer.Transcribe(
                make_request(initial_prompt="Custom prompt"), MagicMock()
            )
        )
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        # Ensure prompt is passed to the underlying model
        assert call_kwargs["initial_prompt"] == "Custom prompt"

    def test_vad_parameters_passed(self, servicer, mock_whisper_model):
        list(servicer.Transcribe(make_request(), MagicMock()))
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["vad_filter"] is True
        assert call_kwargs["vad_parameters"]["min_silence_duration_ms"] == 500

    def test_audio_data_payload_uses_bytesio(self, servicer, mock_whisper_model):
        """Verifies raw 'data' bytes are wrapped in BytesIO."""
        req = MagicMock()
        req.WhichOneof.return_value = "data"  # .proto field is 'data'
        req.data = b"fake wav bytes"
        req.language = "en"
        req.options.initial_prompt = ""

        list(servicer.Transcribe(req, MagicMock()))

        audio_input = mock_whisper_model.transcribe.call_args[0][0]
        assert isinstance(audio_input, io.BytesIO)
        assert audio_input.read() == b"fake wav bytes"

    @patch("servicer.requests.get")
    def test_audio_uri_payload_fetches_file(
        self, mock_get, servicer, mock_whisper_model
    ):
        """Verifies 'uri' is fetched and passed as BytesIO."""
        mock_response = MagicMock()
        mock_response.content = b"downloaded bytes"
        mock_response.headers = {}
        mock_get.return_value.__enter__.return_value = mock_response

        req = MagicMock()
        req.WhichOneof.return_value = "uri"  # .proto field is 'uri'
        req.uri = "https://example.com/audio.wav"
        req.language = "en"
        req.options.initial_prompt = ""

        list(servicer.Transcribe(req, MagicMock()))

        mock_get.assert_called_once_with(
            "https://example.com/audio.wav", timeout=15, stream=True
        )
        audio_input = mock_whisper_model.transcribe.call_args[0][0]
        assert isinstance(audio_input, io.BytesIO)
        assert audio_input.read() == b"downloaded bytes"

    def test_invalid_audio_source_aborts(self, servicer):
        """Verifies abort if no valid field is set."""
        req = MagicMock()
        req.WhichOneof.return_value = None
        context = MagicMock()

        result = list(servicer.Transcribe(req, context))

        assert len(result) == 0
        context.abort.assert_called_once_with(
            grpc.StatusCode.INVALID_ARGUMENT, "No valid audio_source provided"
        )

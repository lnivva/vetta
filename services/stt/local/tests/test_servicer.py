import io
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import grpc
import pytest

from audio import AudioValidationError
from servicer import WhisperServicer
from settings import (
    Settings,
    ServiceConfig,
    ModelConfig,
    InferenceConfig,
    ConcurrencyConfig,
    DiarizationConfig,
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
            address=f"unix://{tmp_dir / 't.sock'}",
            log_level="info",
            max_audio_size_mb=10,
        ),
        model=ModelConfig(
            size="small", download_dir=str(tmp_dir), device="cpu", compute_type="int8"
        ),
        inference=InferenceConfig(**inference_defaults),
        concurrency=ConcurrencyConfig(max_workers=1, cpu_threads=2, num_workers=1),
        diarization=DiarizationConfig(
            model="pyannote/speaker-diarization-3.1",
            max_speakers=0,
            min_speakers=0,
            hf_token="",
            enabled=False,
            device="cuda",
        ),
    )


def _make_mock_options(initial_prompt="", diarization=False, num_speakers=0):
    """Build a mock options object that behaves like a protobuf message."""
    options = MagicMock()
    options.initial_prompt = initial_prompt
    options.diarization = diarization
    options.num_speakers = num_speakers

    def _has_field(name):
        if name == "num_speakers" and num_speakers > 0:
            return True
        return False

    options.HasField = _has_field
    return options


def make_request(path="test.mp3", language="en", initial_prompt=""):
    """
    Create a mocked gRPC request object configured to represent a 'path' payload.
    """
    req = MagicMock()
    req.WhichOneof.return_value = "path"
    req.path = path
    req.language = language
    req.options = _make_mock_options(initial_prompt=initial_prompt)
    return req


@pytest.fixture
def servicer(mock_whisper_model, tmp_path):
    """Create a WhisperServicer with its WhisperModel patched."""
    with patch("servicer.WhisperModel", return_value=mock_whisper_model):
        svc = WhisperServicer(make_settings(tmp_path))
    svc.model = mock_whisper_model
    return svc


def _stub_resolve(servicer, audio_input, log_source="test.mp3", source_type="path"):
    """Replace the resolver so it returns the given audio_input directly."""
    servicer._resolver.resolve = MagicMock(
        return_value=(audio_input, log_source, source_type)
    )


def _stub_preprocessor(servicer):
    """Replace the preprocessor so it passes audio through unchanged."""
    servicer._preprocessor.prepare = MagicMock(
        side_effect=lambda audio, diarize=False: (audio, None)
    )


def _prepare_servicer(
    servicer, audio_input="test.mp3", log_source="test.mp3", source_type="path"
):
    """Wire up both resolver and preprocessor stubs for a standard test."""
    _stub_resolve(servicer, audio_input, log_source, source_type)
    _stub_preprocessor(servicer)


class TestWhisperServicer:
    def test_yields_transcript_chunks(self, servicer):
        _prepare_servicer(servicer)
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert len(chunks) == 1
        assert isinstance(chunks[0], speech_pb2.TranscriptChunk)

    def test_text_is_stripped(self, servicer):
        _prepare_servicer(servicer)
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert chunks[0].text == "Hello world"

    def test_timing_fields(self, servicer):
        _prepare_servicer(servicer)
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert chunks[0].start_time == pytest.approx(0.0)
        assert chunks[0].end_time == pytest.approx(3.5)

    def test_request_initial_prompt_takes_priority(self, servicer, mock_whisper_model):
        _prepare_servicer(servicer)
        list(
            servicer.Transcribe(
                make_request(initial_prompt="Custom prompt"), MagicMock()
            )
        )
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["initial_prompt"] == "Custom prompt"

    def test_vad_parameters_passed(self, servicer, mock_whisper_model):
        _prepare_servicer(servicer)
        list(servicer.Transcribe(make_request(), MagicMock()))
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["vad_filter"] is True
        assert call_kwargs["vad_parameters"]["min_silence_duration_ms"] == 500

    def test_audio_data_payload_flows_through_preprocessor(
        self, servicer, mock_whisper_model
    ):
        """
        Verifies that for a 'data' payload the resolver's output is handed
        to _preprocessor.prepare(), and the preprocessor's return value
        (not the raw resolver output) is what reaches model.transcribe().
        """
        raw_bytes = b"fake wav bytes"
        resolver_output = io.BytesIO(raw_bytes)

        # Resolver returns the BytesIO wrapper
        servicer._resolver.resolve = MagicMock(
            return_value=(resolver_output, "<bytes>", "data")
        )

        # Preprocessor should receive exactly what the resolver returned,
        # and its output is what the model should see.
        preprocessed_audio = MagicMock(name="preprocessed_ndarray")
        servicer._preprocessor.prepare = MagicMock(
            return_value=(preprocessed_audio, None)
        )

        req = MagicMock()
        req.WhichOneof.return_value = "data"
        req.data = raw_bytes
        req.language = "en"
        req.options = _make_mock_options()

        list(servicer.Transcribe(req, MagicMock()))

        # Assert the preprocessor received the resolver's output
        servicer._preprocessor.prepare.assert_called_once()
        prep_args, prep_kwargs = servicer._preprocessor.prepare.call_args
        assert prep_args[0] is resolver_output

        # Assert the model received the preprocessor's output, not the raw BytesIO
        model_audio_arg = mock_whisper_model.transcribe.call_args[0][0]
        assert model_audio_arg is preprocessed_audio

    def test_audio_uri_payload_flows_through_preprocessor(
        self, servicer, mock_whisper_model
    ):
        """
        Verifies that for a 'uri' payload the resolver's output is handed
        to _preprocessor.prepare(), and the preprocessor's return value
        is what reaches model.transcribe().
        """
        downloaded_bytes = b"downloaded bytes"
        resolver_output = io.BytesIO(downloaded_bytes)
        uri = "https://example.com/audio.wav"

        servicer._resolver.resolve = MagicMock(
            return_value=(resolver_output, uri, "uri")
        )

        preprocessed_audio = MagicMock(name="preprocessed_ndarray")
        servicer._preprocessor.prepare = MagicMock(
            return_value=(preprocessed_audio, None)
        )

        req = MagicMock()
        req.WhichOneof.return_value = "uri"
        req.uri = uri
        req.language = "en"
        req.options = _make_mock_options()

        list(servicer.Transcribe(req, MagicMock()))

        # Assert the preprocessor received the resolver's output
        servicer._preprocessor.prepare.assert_called_once()
        prep_args, prep_kwargs = servicer._preprocessor.prepare.call_args
        assert prep_args[0] is resolver_output

        # Assert the model received the preprocessor's output, not the raw BytesIO
        model_audio_arg = mock_whisper_model.transcribe.call_args[0][0]
        assert model_audio_arg is preprocessed_audio

    def test_invalid_audio_source_aborts(self, servicer):
        """Verifies abort if no valid field is set."""
        servicer._resolver.resolve = MagicMock(
            side_effect=AudioValidationError("No valid audio_source provided")
        )

        req = MagicMock()
        req.WhichOneof.return_value = None
        context = MagicMock()

        class _Abort(Exception):
            pass

        context.abort.side_effect = _Abort

        with pytest.raises(_Abort):
            list(servicer.Transcribe(req, context))

        context.abort.assert_called_once_with(
            grpc.StatusCode.INVALID_ARGUMENT, "No valid audio_source provided"
        )

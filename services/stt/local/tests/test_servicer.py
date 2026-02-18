from unittest.mock import MagicMock, patch

import pytest
import speech_pb2
from servicer import WhisperServicer
from settings import Settings, ServiceConfig, ModelConfig, InferenceConfig, ConcurrencyConfig


def make_settings(**inference_overrides) -> Settings:
    inference_defaults = dict(
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
        service=ServiceConfig(socket_path="/tmp/t.sock", log_level="info"),
        model=ModelConfig(size="small", download_dir="/tmp", device="cpu", compute_type="int8"),
        inference=InferenceConfig(**inference_defaults),
        concurrency=ConcurrencyConfig(max_workers=1, cpu_threads=2, num_workers=1),
    )


@pytest.fixture
def servicer(mock_whisper_model):
    with patch("servicer.WhisperModel", return_value=mock_whisper_model):
        svc = WhisperServicer(make_settings())
    svc._model = mock_whisper_model  # keep reference for assertions
    return svc


def make_request(audio_path="/tmp/test.mp3", language="en", initial_prompt=""):
    options = MagicMock()
    options.initial_prompt = initial_prompt
    req = MagicMock()
    req.audio_path = audio_path
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
        assert chunks[0].text == "Hello world"  # stripped

    def test_timing_fields(self, servicer):
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert chunks[0].start_time == pytest.approx(0.0)
        assert chunks[0].end_time == pytest.approx(3.5)

    def test_word_timestamps_included(self, servicer):
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert len(chunks[0].words) == 1
        assert chunks[0].words[0].text == "Hello"

    def test_speaker_id_empty_before_diarization(self, servicer):
        chunks = list(servicer.Transcribe(make_request(), context=MagicMock()))
        assert chunks[0].speaker_id == ""

    def test_request_initial_prompt_takes_priority(self, servicer, mock_whisper_model):
        """Per-request prompt overrides the config default."""
        list(servicer.Transcribe(make_request(initial_prompt="Custom prompt"), MagicMock()))
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["initial_prompt"] == "Custom prompt"

    def test_config_prompt_used_as_fallback(self, mock_whisper_model):
        with patch("servicer.WhisperModel", return_value=mock_whisper_model):
            svc = WhisperServicer(make_settings(initial_prompt="Config fallback"))
        list(svc.Transcribe(make_request(initial_prompt=""), MagicMock()))
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["initial_prompt"] == "Config fallback"

    def test_no_prompt_when_both_empty(self, servicer, mock_whisper_model):
        list(servicer.Transcribe(make_request(initial_prompt=""), MagicMock()))
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["initial_prompt"] is None

    def test_vad_parameters_passed(self, servicer, mock_whisper_model):
        list(servicer.Transcribe(make_request(), MagicMock()))
        call_kwargs = mock_whisper_model.transcribe.call_args.kwargs
        assert call_kwargs["vad_filter"] is True
        assert call_kwargs["vad_parameters"]["min_silence_duration_ms"] == 500

    def test_multiple_segments_yield_multiple_chunks(self, servicer, mock_whisper_model):
        seg2 = MagicMock()
        seg2.start, seg2.end, seg2.text = 3.5, 7.0, "next segment"
        seg2.avg_logprob, seg2.words = -0.2, []
        mock_whisper_model.transcribe.return_value = ([
                                                          mock_whisper_model.transcribe.return_value[0][0],
                                                          seg2,
                                                      ], mock_whisper_model.transcribe.return_value[1])

        chunks = list(servicer.Transcribe(make_request(), MagicMock()))
        assert len(chunks) == 2

    def test_segment_with_no_words(self, servicer, mock_whisper_model):
        seg = MagicMock()
        seg.start, seg.end, seg.text = 0.0, 2.0, "no words segment"
        seg.avg_logprob, seg.words = -0.1, None
        mock_whisper_model.transcribe.return_value = ([seg], mock_whisper_model.transcribe.return_value[1])

        chunks = list(servicer.Transcribe(make_request(), MagicMock()))
        assert chunks[0].words == []  # no crash, empty list

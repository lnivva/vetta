"""
gRPC Servicer for the Whisper Speech-to-Text service.

This module contains the core servicer class that interfaces with the
faster-whisper library to process audio transcription requests via gRPC.
"""

import io
import logging

import grpc
import requests
from faster_whisper import WhisperModel

from settings import Settings
from speech import speech_pb2_grpc, speech_pb2

logger = logging.getLogger(__name__)


class WhisperServicer(speech_pb2_grpc.SpeechToTextServicer):
    """
    gRPC servicer for handling Speech-to-Text operations.

    Attributes:
        inference (InferenceSettings): Configuration settings for the inference process.
        model (WhisperModel): The loaded faster-whisper model instance.
    """

    def __init__(self, settings: Settings):
        """
        Create a WhisperServicer configured from application Settings.

        Initializes the servicer's inference configuration, computes the maximum allowed audio size (in bytes) from service.max_audio_size_mb, and instantiates the WhisperModel using model and concurrency settings from the provided Settings.

        Parameters:
            settings (Settings): Application settings containing model parameters, service limits, and concurrency configuration.
        """
        s = settings
        self.inference = s.inference

        self.max_audio_bytes = s.service.max_audio_size_mb * 1024 * 1024

        self.model = WhisperModel(
            s.model.size,
            device=s.model.device,
            compute_type=s.model.compute_type,
            download_root=s.model.download_dir,
            num_workers=s.concurrency.num_workers,
            cpu_threads=s.concurrency.cpu_threads,
        )

    def Transcribe(self, request, context):
        """
        Stream transcription chunks for the provided audio input.

        Accepts audio via path, raw bytes (data), or URI, applies inference settings (language, VAD, beam size, word timestamps, thresholds, and initial prompt), and yields speech_pb2.TranscriptChunk messages for each transcription segment with per-word timing and confidence.

        Returns:
            Generator[speech_pb2.TranscriptChunk]: A stream of transcript chunks corresponding to recognized segments.

        Raises:
            grpc.RpcError: Aborts with INVALID_ARGUMENT when no valid audio source is provided, when audio exceeds the configured maximum size, or when fetching a remote URI fails.
        """
        inf = self.inference
        prompt = request.options.initial_prompt or inf.initial_prompt or None
        audio_source_type = request.WhichOneof("audio_source")

        audio_input = None

        if audio_source_type == "path":
            audio_input = request.path
            log_source = request.path

        elif audio_source_type == "data":
            if len(request.data) > self.max_audio_bytes:
                return context.abort(
                    grpc.StatusCode.INVALID_ARGUMENT,
                    f"Audio data exceeds maximum size of {self.max_audio_bytes} bytes",
                )
            audio_input = io.BytesIO(request.data)
            log_source = "<bytes_payload>"

        elif audio_source_type == "uri":
            try:
                with requests.get(request.uri, timeout=15, stream=True) as response:
                    response.raise_for_status()

                    content_length = response.headers.get("Content-Length")
                    if content_length and int(content_length) > self.max_audio_bytes:
                        return context.abort(
                            grpc.StatusCode.INVALID_ARGUMENT,
                            f"Remote audio file exceeds maximum size of {self.max_audio_bytes} bytes",
                        )

                    audio_input = io.BytesIO(response.content)
                log_source = request.uri
            except requests.RequestException as e:
                logger.exception("Failed to fetch audio from URI")
                return context.abort(
                    grpc.StatusCode.INVALID_ARGUMENT, f"Failed to fetch audio URI: {e}"
                )

        else:
            return context.abort(
                grpc.StatusCode.INVALID_ARGUMENT, "No valid audio_source provided"
            )

        segments, info = self.model.transcribe(
            audio_input,
            language=request.language or None,
            beam_size=inf.beam_size,
            vad_filter=inf.vad_filter,
            vad_parameters={"min_silence_duration_ms": inf.vad_min_silence_ms},
            word_timestamps=inf.word_timestamps,
            initial_prompt=prompt,
            no_speech_threshold=inf.no_speech_threshold,
            log_prob_threshold=inf.log_prob_threshold,
            compression_ratio_threshold=inf.compression_ratio_threshold,
        )

        logger.info(
            "Transcription started",
            extra={
                "language": info.language,
                "language_probability": round(info.language_probability, 2),
                "audio_source_type": audio_source_type,
                "audio_source": log_source,
            },
        )

        for segment in segments:
            yield speech_pb2.TranscriptChunk(
                start_time=segment.start,
                end_time=segment.end,
                text=segment.text.strip(),
                speaker_id="",
                confidence=segment.avg_logprob,
                words=[
                    speech_pb2.Word(
                        start_time=w.start,
                        end_time=w.end,
                        text=w.word,
                        confidence=w.probability,
                    )
                    for w in (segment.words or [])
                ],
            )

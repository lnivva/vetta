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
        Initializes the WhisperServicer with the specified settings.

        Args:
            settings (Settings): The application settings containing model
                configuration, concurrency limits, and inference parameters.
        """
        s = settings
        self.inference = s.inference
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
        Processes an audio file and yields transcription chunks as a gRPC stream.

        Args:
            request (speech_pb2.TranscribeRequest): The gRPC request containing the
                audio source (path, URI, or bytes) and transcription options.
            context (grpc.ServicerContext): The gRPC context for the RPC.

        Yields:
            speech_pb2.TranscriptChunk: A chunk of the transcribed text, including
                timing, confidence scores, and word-level timestamps.
        """
        inf = self.inference
        prompt = request.options.initial_prompt or inf.initial_prompt or None

        audio_source_type = request.WhichOneof("audio_source")

        if audio_source_type == "audio_path":
            audio_input = request.audio_path
            log_source = request.audio_path

        elif audio_source_type == "audio_data":
            audio_input = io.BytesIO(request.audio_data)
            log_source = "<bytes_payload>"

        elif audio_source_type == "audio_uri":
            try:
                response = requests.get(request.audio_uri, timeout=15)
                response.raise_for_status()
                audio_input = io.BytesIO(response.content)
                log_source = request.audio_uri
            except requests.RequestException as e:
                logger.error(f"Failed to fetch audio from URI: {e}")
                return context.abort(grpc.StatusCode.INVALID_ARGUMENT, f"Failed to fetch audio URI: {e}")

        else:
            return context.abort(grpc.StatusCode.INVALID_ARGUMENT, "No valid audio_source provided")

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
                "audio_source": log_source
            }
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

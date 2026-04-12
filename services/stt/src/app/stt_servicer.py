import logging
import grpc

from src.generated.speech import speech_pb2_grpc, speech_pb2

from src.core.audio import (
    AudioResolver,
    AudioValidationError,
    AudioFetchError,
    AudioDecodeError,
)
from src.core.settings import Settings

from src.stt.engine import TranscriptionEngine, INFERENCE_ERRORS

logger = logging.getLogger(__name__)


class SpeechToTextServicer(speech_pb2_grpc.SpeechToTextServicer):
    """
    gRPC Adapter for the Transcription Engine.
    Handles proto mapping, audio resolution, and network-level error handling.
    """

    def __init__(self, settings: Settings):
        self._resolver = AudioResolver(
            max_bytes=settings.service.max_audio_size_mb * 1024 * 1024,
        )
        self._engine = TranscriptionEngine(settings)

    def Transcribe(self, request, context):
        """
        Unpack the gRPC request, fetch the audio, and stream the engine's response.
        """
        try:
            audio_data, log_source, source_type = self._resolver.resolve(request)
        except (AudioValidationError, AudioFetchError) as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

        options = request.options
        diarize = options.diarization
        num_speakers = options.num_speakers if options.HasField("num_speakers") else 0
        language = request.language or None
        prompt = options.initial_prompt or None

        try:
            chunk_generator = self._engine.transcribe(
                audio_data=audio_data,
                diarize=diarize,
                num_speakers=num_speakers,
                language=language,
                initial_prompt=prompt,
            )

            for domain_chunk in chunk_generator:
                yield self._map_to_proto(domain_chunk)

        except ValueError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
        except AudioDecodeError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
        except INFERENCE_ERRORS:
            logger.exception("STT Engine failed")
            context.abort(grpc.StatusCode.INTERNAL, "Transcription failed")
        except Exception:
            logger.exception("Unexpected error in STT Servicer")
            context.abort(grpc.StatusCode.INTERNAL, "An unexpected error occurred.")

    @staticmethod
    def _map_to_proto(chunk) -> speech_pb2.TranscriptChunk:
        """
        Maps the domain TranscriptChunkResult to the Protobuf message.
        """
        return speech_pb2.TranscriptChunk(
            start_time=chunk.start_time,
            end_time=chunk.end_time,
            text=chunk.text,
            speaker_id=chunk.speaker_id,
            confidence=chunk.confidence,
            words=[
                speech_pb2.Word(
                    start_time=w.start_time,
                    end_time=w.end_time,
                    text=w.text,
                    confidence=w.confidence,
                    speaker_id=w.speaker_id,
                )
                for w in chunk.words
            ],
        )

from faster_whisper import WhisperModel

import speech_pb2
import speech_pb2_grpc
from settings import Settings


class WhisperServicer(speech_pb2_grpc.SpeechToTextServicer):
    def __init__(self, settings: Settings):
        """
        Initialize the servicer with Settings, store inference configuration, and construct the WhisperModel used for transcription.
        
        Parameters:
            settings (Settings): Application settings providing `inference` defaults and model/concurrency configuration. The constructor stores `settings.inference` to self.inference and creates a WhisperModel configured from `settings.model` (size, device, compute_type, download_dir) and `settings.concurrency` (num_workers, cpu_threads).
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
        Stream transcript chunks for the provided audio request.
        
        Parameters:
            request: gRPC request containing:
                - audio_path (str): Path to the input audio file to transcribe.
                - language (str): Optional override for the audio language.
                - options.initial_prompt (str): Optional initial prompt to guide transcription.
            context: gRPC context (not documented further).
        
        Returns:
            generator: Yields speech_pb2.TranscriptChunk messages for each transcription segment. Each TranscriptChunk includes:
                - start_time, end_time: segment boundaries in seconds.
                - text: segment text (trimmed).
                - speaker_id: speaker identifier (empty if not provided).
                - confidence: segment confidence score.
                - words: list of speech_pb2.Word entries with start_time, end_time, text, and confidence for per-word timestamps.
        """
        inf = self.inference
        prompt = request.options.initial_prompt or inf.initial_prompt or None

        segments, info = self.model.transcribe(
            request.audio_path,
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

        print(f"[transcribe] lang={info.language} p={info.language_probability:.2f} file={request.audio_path}")

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
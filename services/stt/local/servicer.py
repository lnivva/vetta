import speech_pb2
import speech_pb2_grpc
from faster_whisper import WhisperModel

from settings import Settings


class WhisperServicer(speech_pb2_grpc.SpeechToTextServicer):
    def __init__(self, settings: Settings):
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

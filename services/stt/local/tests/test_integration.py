"""
Spins up a real gRPC server over a Unix domain socket.
WhisperModel is mocked — no GPU/model download needed.
Tests the full request/response cycle.
"""
import os
import threading
from concurrent import futures
from unittest.mock import patch

import grpc
import pytest
import speech_pb2
import speech_pb2_grpc
from servicer import WhisperServicer
from settings import load_settings


@pytest.fixture(scope="module")
def grpc_server(tmp_path_factory, mock_whisper_model):
    # Use a short fixed socket path — pytest tmp paths exceed the 104-char UDS limit on macOS
    """
    Start a temporary gRPC server bound to a Unix domain socket for test usage and yield the socket path.
    
    This fixture:
    - Writes a temporary TOML config that points the service to a short Unix socket path.
    - Patches servicer.WhisperModel to return the provided mock_whisper_model.
    - Instantiates WhisperServicer, registers it on a real grpc.Server, binds and starts the server, and sets the socket file permissions to 0o600.
    - On teardown, stops the server and removes the socket file if present.
    
    Parameters:
        tmp_path_factory: pytest tmp_path_factory used to create a temporary config directory.
        mock_whisper_model: Mock object to be returned by the patched WhisperModel.
    
    Returns:
        sock (str): Filesystem path to the Unix domain socket the server is bound to.
    """
    sock = f"/tmp/whisper_test_{os.getpid()}.sock"

    config = tmp_path_factory.mktemp("config") / "config.toml"
    config.write_text(f"""\
        [service]
        socket_path = "{sock}"
        log_level   = "info"

        [model]
        size         = "small"
        download_dir = "/tmp"
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

    settings = load_settings(config)

    with patch("servicer.WhisperModel", return_value=mock_whisper_model):
        svc = WhisperServicer(settings)

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    speech_pb2_grpc.add_SpeechToTextServicer_to_server(svc, server)
    server.add_insecure_port(f"unix://{sock}")
    server.start()
    os.chmod(sock, 0o600)

    yield sock

    server.stop(grace=1)
    if os.path.exists(sock):
        os.unlink(sock)


@pytest.fixture(scope="module")
def grpc_client(grpc_server):
    """
    Provide a SpeechToTextStub connected to the test gRPC server socket.
    
    Yields a gRPC client stub connected to the server's Unix domain socket. The underlying channel is closed when the fixture is torn down.
    
    Parameters:
        grpc_server (str): Path to the server's Unix domain socket.
    
    Returns:
        client (speech_pb2_grpc.SpeechToTextStub): gRPC client stub connected to the server.
    """
    channel = grpc.insecure_channel(f"unix://{grpc_server}")
    client = speech_pb2_grpc.SpeechToTextStub(channel)
    yield client
    channel.close()


def make_grpc_request(audio_path="/tmp/fake.mp3", language="en"):
    """
    Builds a TranscribeRequest for the Whisper gRPC service.
    
    Parameters:
        audio_path (str): Filesystem path to the audio file to transcribe. Defaults to "/tmp/fake.mp3".
        language (str): Language code for transcription (e.g., "en"). Defaults to "en".
    
    Returns:
        speech_pb2.TranscribeRequest: Request populated with the given audio_path and language. The request's TranscribeOptions are set to diarization=False, num_speakers=2, and initial_prompt="".
    """
    return speech_pb2.TranscribeRequest(
        audio_path=audio_path,
        language=language,
        options=speech_pb2.TranscribeOptions(
            diarization=False,
            num_speakers=2,
            initial_prompt="",
        ),
    )


class TestGrpcIntegration:
    def test_transcribe_returns_chunks(self, grpc_client):
        chunks = list(grpc_client.Transcribe(make_grpc_request()))
        assert len(chunks) >= 1

    def test_chunk_has_text(self, grpc_client):
        chunks = list(grpc_client.Transcribe(make_grpc_request()))
        assert chunks[0].text == "Hello world"

    def test_chunk_has_timing(self, grpc_client):
        chunks = list(grpc_client.Transcribe(make_grpc_request()))
        assert chunks[0].start_time == pytest.approx(0.0)
        assert chunks[0].end_time == pytest.approx(3.5)

    def test_chunk_has_words(self, grpc_client):
        chunks = list(grpc_client.Transcribe(make_grpc_request()))
        assert len(chunks[0].words) == 1
        assert chunks[0].words[0].text == "Hello"

    def test_concurrent_requests(self, grpc_client):
        """
        Verifies the server handles two concurrent Transcribe requests without error.
        
        Starts two threads that each call Transcribe via the provided grpc_client and asserts no exceptions occurred and both calls produced results.
        """
        results = [None, None]
        errors = []

        def call(index):
            """
            Invoke the Transcribe RPC and store its streamed response into the shared results list at the given index.
            
            This function collects all streamed chunks from grpc_client.Transcribe(make_grpc_request()) into a list and assigns that list to results[index]. If an exception occurs during the RPC or iteration, the exception is appended to the shared errors list.
            
            Parameters:
                index (int): Index in the shared `results` list where the response list will be stored.
            """
            try:
                results[index] = list(grpc_client.Transcribe(make_grpc_request()))
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=call, args=(i,)) for i in range(2)]
        for t in threads: t.start()
        for t in threads: t.join()

        assert not errors
        assert all(r is not None for r in results)

    def test_response_is_streaming(self, grpc_client):
        """
        Verify that Transcribe returns a streaming iterator.
        
        Asserts the response implements the iteration protocol by having both `__iter__` and `__next__`.
        """
        response = grpc_client.Transcribe(make_grpc_request())
        # grpc streaming response is an iterator, not a list
        assert hasattr(response, "__iter__")
        assert hasattr(response, "__next__")
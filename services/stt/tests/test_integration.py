import os
import shutil
import tempfile
import threading
from concurrent import futures
from unittest.mock import patch

import grpc
import pytest

from servicer import WhisperServicer
from settings import load_settings
from speech import speech_pb2_grpc, speech_pb2


@pytest.fixture(scope="module")
def grpc_server(tmp_path_factory, mock_whisper_model):
    """
    Create and start a gRPC server bound to a temporary Unix-domain socket and yield the socket path for tests.

    The server is configured with a temporary TOML config, replaces the real WhisperModel with the provided mock, registers the WhisperServicer, and listens on a Unix socket. On teardown the server is stopped and socket files are removed.

    Parameters:
        tmp_path_factory: pytest tmp_path_factory used to create temporary configuration files and directories.
        mock_whisper_model: Mock instance used to replace the WhisperModel during tests.

    Returns:
        str: Filesystem path to the Unix-domain socket where the gRPC server is listening.
    """
    sock_dir = tempfile.mkdtemp(prefix="whisper_test_", dir="/tmp")
    sock = os.path.join(sock_dir, "grpc.sock")

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
    if os.path.isdir(sock_dir):
        shutil.rmtree(sock_dir, ignore_errors=True)


@pytest.fixture(scope="module")
def grpc_client(grpc_server):
    """
    Provide a SpeechToTextStub gRPC client connected to the Unix-domain socket created by the grpc_server fixture.

    Parameters:
        grpc_server (str): Filesystem path to the Unix-domain socket exposed by the grpc_server fixture.

    Returns:
        speech_pb2_grpc.SpeechToTextStub: A gRPC client bound to the socket. The underlying channel is closed when the fixture is torn down.
    """
    channel = grpc.insecure_channel(f"unix://{grpc_server}")
    client = speech_pb2_grpc.SpeechToTextStub(channel)
    yield client
    channel.close()


def make_grpc_request(audio_path=None, language="en"):
    """
    Builds a TranscribeRequest for gRPC tests.
    """
    if audio_path is None:
        fd, audio_path = tempfile.mkstemp(
            prefix="whisper_test_", suffix=".mp3", dir="/tmp"
        )
        os.close(fd)

    return speech_pb2.TranscribeRequest(
        path=audio_path,
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
        Ensure two concurrent Transcribe requests complete without errors.

        Runs two threads that call the gRPC Transcribe method simultaneously and asserts no RpcError was raised and both calls produced results.
        """
        results = [None, None]
        errors = []

        def call(index):
            """
            Perform a Transcribe RPC and store the streamed chunks or record any RpcError.

            Parameters:
                index (int): Index into the shared `results` list where the list of response chunks will be stored. On RpcError, the exception is appended to the shared `errors` list.

            Returns:
                None
            """
            try:
                results[index] = list(grpc_client.Transcribe(make_grpc_request()))
            except grpc.RpcError as e:
                errors.append(e)

        threads = [threading.Thread(target=call, args=(i,)) for i in range(2)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        assert not errors
        assert all(r is not None for r in results)

    def test_response_is_streaming(self, grpc_client):
        """Verify we get an iterator (streaming), not a single response."""
        response = grpc_client.Transcribe(make_grpc_request())
        assert hasattr(response, "__iter__")
        assert hasattr(response, "__next__")

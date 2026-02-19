import argparse
import os
from concurrent import futures
from pathlib import Path

import grpc

import speech_pb2_grpc
from servicer import WhisperServicer
from settings import load_settings


def serve(config_path: str):
    """
    Start and run the Speech-to-Text gRPC server using configuration from the provided file.
    
    Loads settings from config_path, starts a gRPC server that serves the WhisperServicer on the Unix domain socket defined by the settings, ensures the socket file has owner read/write permissions (0600), and blocks until the server terminates.
    
    Parameters:
        config_path (str): Path to the configuration file used to load server settings.
    """
    settings = load_settings(config_path)

    socket_path = settings.service.socket_path

    if os.path.exists(socket_path):
        os.unlink(socket_path)

    server = grpc.server(
        futures.ThreadPoolExecutor(max_workers=settings.concurrency.max_workers)
    )
    speech_pb2_grpc.add_SpeechToTextServicer_to_server(
        WhisperServicer(settings), server
    )
    server.add_insecure_port(f"unix://{socket_path}")
    server.start()
    os.chmod(socket_path, 0o600)

    print(f"[whisper] ready on {socket_path}")
    server.wait_for_termination()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--config",
        default=str(Path(__file__).parent / "config.toml"),
        help="Path to config.toml",
    )
    args = parser.parse_args()
    serve(args.config)
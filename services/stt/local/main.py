"""
Entry point for the Whisper Speech-to-Text gRPC service.

This script configures structured logging, initializes the gRPC server 
over a Unix domain socket, and handles the service lifecycle.
"""

import argparse
import logging
import os
from concurrent import futures
from pathlib import Path

import grpc
from pythonjsonlogger.json import JsonFormatter

from servicer import WhisperServicer
from settings import load_settings
from speech import speech_pb2_grpc

logger = logging.getLogger(__name__)


def setup_logging():
    """
    Configures the root logger to output structured JSON.
    
    This ensures that all logs across the application are formatted consistently 
    as JSON strings, making them compatible with log aggregation systems.
    """
    root_logger = logging.getLogger()
    root_logger.setLevel(logging.INFO)

    # Prevent adding multiple handlers if called multiple times
    if not root_logger.handlers:
        log_handler = logging.StreamHandler()

        # Define the fields you want in every log entry using the updated class
        formatter = JsonFormatter(
            '%(asctime)s %(levelname)s %(name)s %(message)s'
        )
        log_handler.setFormatter(formatter)
        root_logger.addHandler(log_handler)


def serve(config_path: str):
    """
    Initializes and starts the gRPC server.

    This function sets up the required Unix domain socket, binds the 
    WhisperServicer to the gRPC server, and keeps the server running 
    until it is terminated.

    Args:
        config_path (str): The file path to the TOML configuration file.
    """
    setup_logging()

    settings = load_settings(config_path)
    socket_path = settings.service.socket_path

    # Clean up a stale socket file if it exists
    if os.path.exists(socket_path):
        os.unlink(socket_path)

    server = grpc.server(
        futures.ThreadPoolExecutor(max_workers=settings.concurrency.max_workers)
    )
    speech_pb2_grpc.add_SpeechToTextServicer_to_server(
        WhisperServicer(settings), server
    )

    # Bind to Unix domain socket instead of TCP port
    server.add_insecure_port(f"unix://{socket_path}")
    server.start()

    # Restrict socket permissions for security
    os.chmod(socket_path, 0o600)

    logger.info("Service started and ready", extra={"socket_path": socket_path})

    server.wait_for_termination()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run the Whisper gRPC STT service."
    )
    parser.add_argument(
        "--config",
        default=str(Path(__file__).parent / "config.toml"),
        help="Path to config.toml",
    )
    args = parser.parse_args()
    serve(args.config)

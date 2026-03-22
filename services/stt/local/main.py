"""
Entry point for the Whisper Speech-to-Text gRPC service.

This script configures structured logging, initializes the gRPC server
over a Unix domain socket, and handles the service lifecycle.
"""

import argparse
import logging
import os
import signal
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
    Configure the root logger to emit structured JSON logs.

    Sets the root logger level to INFO and, if the root logger has no handlers,
    adds a StreamHandler formatted with a JsonFormatter containing the fields:
    asctime, levelname, name, and message. This prevents adding duplicate handlers
    when called multiple times.
    """
    root_logger = logging.getLogger()
    root_logger.setLevel(logging.INFO)

    # Prevent adding multiple handlers if called multiple times
    if not root_logger.handlers:
        log_handler = logging.StreamHandler()

        # Define the fields you want in every log entry using the updated class
        formatter = JsonFormatter("%(asctime)s %(levelname)s %(name)s %(message)s")
        log_handler.setFormatter(formatter)
        root_logger.addHandler(log_handler)


def serve(config_path: str):
    setup_logging()

    settings = load_settings(config_path)
    address = settings.service.address
    socket_path = settings.service.socket_path

    assert socket_path is not None, "Socket path should not be None"

    if socket_path and os.path.exists(socket_path):
        os.unlink(socket_path)

    server = grpc.server(
        futures.ThreadPoolExecutor(max_workers=settings.concurrency.max_workers)
    )
    speech_pb2_grpc.add_SpeechToTextServicer_to_server(
        WhisperServicer(settings), server
    )

    if settings.service.is_unix_socket:
        server.add_insecure_port(address)
        server.start()
        os.chmod(socket_path, 0o600)
    else:
        # TODO: harden security (ssl/tls mtls)
        server.add_insecure_port(address)
        server.start()

    logger.info("Service started", extra={"address": address})

    def handle_shutdown(signum, frame):
        logger.info(f"Received signal {signum}, shutting down...")
        stop_event = server.stop(grace=10)
        stop_event.wait()
        logger.info("Shutdown complete.")

    signal.signal(signal.SIGTERM, handle_shutdown)
    signal.signal(signal.SIGINT, handle_shutdown)

    server.wait_for_termination()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run the Whisper gRPC STT service.")
    parser.add_argument(
        "--config",
        default=str(Path(__file__).parent / "config.toml"),
        help="Path to config.toml",
    )
    args = parser.parse_args()
    serve(args.config)

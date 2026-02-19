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
    Initializes and starts the gRPC server with graceful shutdown handling.
    """
    setup_logging()

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

    logger.info("Service started", extra={"socket_path": socket_path})

    # --- Graceful Shutdown Logic ---
    def handle_shutdown(signum, frame):
        """
        Triggered on SIGTERM or SIGINT.
        Shuts down the server with a grace period for active RPCs.
        """
        logger.info(f"Received signal {signum}, shutting down...")
        # server.stop(grace) returns an event that we can wait for
        stop_event = server.stop(grace=10)
        stop_event.wait()
        logger.info("Shutdown complete.")

    # Register signals
    signal.signal(signal.SIGTERM, handle_shutdown)
    signal.signal(signal.SIGINT, handle_shutdown)

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

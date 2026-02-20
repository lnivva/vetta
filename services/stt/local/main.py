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
    """
    Start the Whisper gRPC service using settings from the given configuration file.

    Loads settings from config_path, creates and starts a gRPC server bound to the Unix domain socket specified by settings.service.socket_path, registers the WhisperServicer, sets the socket file permissions to owner read/write (0o600), and installs SIGTERM/SIGINT handlers that perform a graceful shutdown with a 10-second grace period.

    Parameters:
        config_path (str): Filesystem path to the configuration file used to load service settings.
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
        Handle termination signals by stopping the gRPC server and waiting up to 10 seconds for active RPCs to finish.

        Blocks until shutdown completes.
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
    parser = argparse.ArgumentParser(description="Run the Whisper gRPC STT service.")
    parser.add_argument(
        "--config",
        default=str(Path(__file__).parent / "config.toml"),
        help="Path to config.toml",
    )
    args = parser.parse_args()
    serve(args.config)

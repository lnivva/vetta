"""
Audio loading, validation, and preprocessing.

Handles resolution of audio from multiple sources (file paths, raw bytes,
remote URIs) and normalisation to the format expected by downstream
models (16 kHz mono float32 numpy arrays).
"""

import io
import ipaddress
import logging
import socket
import subprocess
from urllib.parse import urlparse

import numpy as np
import requests

logger = logging.getLogger(__name__)

# Whisper expects 16 kHz mono audio
SAMPLE_RATE = 16000

# Schemes permitted for remote audio fetches.
_ALLOWED_SCHEMES = {"http", "https"}

# Maximum number of redirects we will follow (each hop is re-validated).
_MAX_REDIRECTS = 5


class AudioValidationError(Exception):
    """Raised when audio input fails a validation check."""


class AudioFetchError(Exception):
    """Raised when a remote audio URI cannot be retrieved."""


class AudioDecodeError(Exception):
    """Raised when ffmpeg fails to decode audio bytes."""


def _is_public_ip(address: str) -> bool:
    """Return True only when *address* resolves exclusively to public IPs.

    Performs DNS resolution and rejects any result that falls inside a
    private, loopback, link-local, multicast, or otherwise reserved
    range.  If the hostname resolves to *any* non-public address the
    whole request is denied — this prevents DNS-rebinding and dual-stack
    bypass attacks.
    """
    try:
        infos = socket.getaddrinfo(address, None, socket.AF_UNSPEC, socket.SOCK_STREAM)
    except socket.gaierror:
        return False

    if not infos:
        return False

    for family, _type, _proto, _canonname, sockaddr in infos:
        ip_str = sockaddr[0]
        try:
            ip = ipaddress.ip_address(ip_str)
        except ValueError:
            return False

        if (
            ip.is_private
            or ip.is_loopback
            or ip.is_link_local
            or ip.is_multicast
            or ip.is_reserved
            or ip.is_unspecified
        ):
            return False

    return True


def _validate_uri(uri: str) -> None:
    """Validate that *uri* points to a public HTTP(S) destination.

    Raises:
        AudioValidationError: If the scheme is disallowed, the host
            cannot be resolved, or any resolved IP is non-public.
    """
    parsed = urlparse(uri)

    if parsed.scheme.lower() not in _ALLOWED_SCHEMES:
        raise AudioValidationError(
            f"URI scheme {parsed.scheme!r} is not allowed. "
            f"Permitted schemes: {_ALLOWED_SCHEMES}"
        )

    hostname = parsed.hostname
    if not hostname:
        raise AudioValidationError("URI must contain a valid hostname")

    try:
        literal_ip = ipaddress.ip_address(hostname)
        if (
            literal_ip.is_private
            or literal_ip.is_loopback
            or literal_ip.is_link_local
            or literal_ip.is_multicast
            or literal_ip.is_reserved
            or literal_ip.is_unspecified
        ):
            raise AudioValidationError(f"URI resolves to non-public IP {literal_ip}")
        return
    except ValueError:
        pass

    if not _is_public_ip(hostname):
        raise AudioValidationError(
            f"URI host {hostname!r} resolves to a non-public or unresolvable IP address"
        )


class AudioResolver:
    """
    Resolves and validates audio from gRPC request sources.

    Handles three audio source types:
      - ``path``:  local file path (passed through as-is)
      - ``data``:  inline bytes payload
      - ``uri``:   remote URL (downloaded with size limits)

    Parameters:
        max_bytes: Maximum allowed audio size in bytes.
    """

    def __init__(self, max_bytes: int):
        self._max_bytes = max_bytes

    def resolve(self, request) -> tuple[str | bytes, str, str]:
        """
        Extract audio content from a TranscribeRequest.

        Returns:
            tuple of (audio, log_source, source_type) where *audio* is
            either a file-path ``str`` or raw ``bytes``.

        Raises:
            AudioValidationError: If the payload exceeds size limits or
                no valid source is provided.
            AudioFetchError: If a remote URI cannot be retrieved.
        """
        source_type = request.WhichOneof("audio_source")

        if source_type == "path":
            return request.path, request.path, source_type

        if source_type == "data":
            self._check_size(len(request.data))
            return bytes(request.data), "<bytes_payload>", source_type

        if source_type == "uri":
            return self._fetch_uri(request.uri), request.uri, source_type

        raise AudioValidationError("No valid audio_source provided")

    def _check_size(self, size: int) -> None:
        if size > self._max_bytes:
            raise AudioValidationError(
                f"Audio data exceeds maximum size of {self._max_bytes} bytes"
            )

    def _fetch_uri(self, uri: str) -> bytes:
        """
        Download audio from a remote URI with SSRF protection and
        streaming size checks.

        The method:
        1. Validates the scheme (HTTP/HTTPS only) and resolved IPs of
           the initial URI *before* opening a connection.
        2. Disables automatic redirect-following so each hop can be
           independently validated against the same rules.
        3. Enforces the configured byte-size limit during streaming.

        Raises:
            AudioValidationError: If the URI targets a non-public host
                or the download exceeds the size limit.
            AudioFetchError: On any network or HTTP error.
        """
        current_uri = uri

        try:
            for _redirect in range(_MAX_REDIRECTS + 1):
                _validate_uri(current_uri)

                response = requests.get(
                    current_uri,
                    timeout=15,
                    stream=True,
                    allow_redirects=False,
                )

                if response.is_redirect or response.is_permanent_redirect:
                    redirect_target = response.headers.get("Location")
                    if not redirect_target:
                        raise AudioFetchError(
                            "Received redirect with no Location header"
                        )
                    redirect_parsed = urlparse(redirect_target)
                    if not redirect_parsed.scheme:
                        from urllib.parse import urljoin

                        redirect_target = urljoin(current_uri, redirect_target)
                    current_uri = redirect_target
                    response.close()
                    continue

                response.raise_for_status()

                content_length = response.headers.get("Content-Length")
                if content_length:
                    self._check_size(int(content_length))

                chunks = []
                downloaded = 0
                for chunk in response.iter_content(chunk_size=1024 * 1024):
                    downloaded += len(chunk)
                    self._check_size(downloaded)
                    chunks.append(chunk)

                return b"".join(chunks)

            raise AudioFetchError(
                f"Too many redirects (>{_MAX_REDIRECTS}) fetching audio URI"
            )

        except (AudioValidationError, AudioFetchError):
            raise
        except requests.RequestException as exc:
            logger.exception("Failed to fetch audio from URI")
            raise AudioFetchError(f"Failed to fetch audio URI: {exc}") from exc


class AudioPreprocessor:
    """
    Decodes and normalises audio for consumption by Whisper and pyannote.

    Whisper (via CTranslate2) performs best when given a pre-decoded
    float32 numpy array — this avoids its internal ffmpeg subprocess
    on every call.

    Pyannote expects either a file path or raw encoded bytes that it
    decodes internally.

    Parameters:
        sample_rate: Target sample rate in Hz (default 16 000).
    """

    def __init__(self, sample_rate: int = SAMPLE_RATE):
        self._sample_rate = sample_rate

    def decode_to_float32(self, audio_bytes: bytes) -> np.ndarray:
        """
        Decode encoded audio bytes to a 16 kHz mono float32 numpy array.

        Uses ffmpeg to handle any input codec (mp3, ogg, wav, flac, …).

        Parameters:
            audio_bytes: Raw encoded audio data.

        Returns:
            np.ndarray: 1-D float32 array normalised to [-1.0, 1.0].

        Raises:
            AudioDecodeError: If ffmpeg exits with a non-zero status.
        """
        cmd = [
            "ffmpeg",
            "-nostdin",
            "-threads",
            "0",
            "-i",
            "pipe:0",
            "-ar",
            str(self._sample_rate),
            "-ac",
            "1",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "pipe:1",
        ]

        process = subprocess.run(
            cmd,
            input=audio_bytes,
            capture_output=True,
        )

        if process.returncode != 0:
            raise AudioDecodeError(
                f"ffmpeg exited with code {process.returncode}: "
                f"{process.stderr.decode().strip()}"
            )

        return (
            np.frombuffer(process.stdout, dtype=np.int16).astype(np.float32) / 32768.0
        )

    @staticmethod
    def to_bytesio(audio_bytes: bytes) -> io.BytesIO:
        """
        Wrap raw bytes in a seekable BytesIO for pyannote.

        Parameters:
            audio_bytes: Raw encoded audio data.

        Returns:
            io.BytesIO: Seekable stream at position 0.
        """
        return io.BytesIO(audio_bytes)

    def prepare(
        self, audio: str | bytes, *, diarize: bool = False
    ) -> tuple[np.ndarray | str, io.BytesIO | str | None]:
        """
        Prepare audio for transcription and optional diarization.

        For byte inputs, decodes once and produces:
          - a float32 numpy array for Whisper
          - a BytesIO of the original bytes for pyannote (if diarizing)

        For file paths, both pipelines can read the file directly.

        Parameters:
            audio: Either a file path (str) or raw encoded bytes.
            diarize: Whether diarization input is needed.

        Returns:
            (whisper_input, diarize_input) where diarize_input is None
            when *diarize* is False.
        """
        if isinstance(audio, bytes):
            whisper_input = self.decode_to_float32(audio)
            diar_input = self.to_bytesio(audio) if diarize else None
            return whisper_input, diar_input

        # File path — both pipelines read it directly
        return audio, audio if diarize else None

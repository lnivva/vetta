import os
import platform
import subprocess
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

# ── Types ─────────────────────────────────────────────────────────────────────

Device = Literal["cuda", "cpu"]
ComputeType = Literal["float16", "int8_float16", "int8", "float32"]


@dataclass
class ServiceConfig:
    socket_path: str
    log_level: str
    max_audio_size_mb: int


@dataclass
class ModelConfig:
    size: str
    download_dir: str
    device: Device
    compute_type: ComputeType


@dataclass
class InferenceConfig:
    beam_size: int
    vad_filter: bool
    vad_min_silence_ms: int
    no_speech_threshold: float
    log_prob_threshold: float
    compression_ratio_threshold: float
    word_timestamps: bool
    initial_prompt: str


@dataclass
class ConcurrencyConfig:
    max_workers: int
    cpu_threads: int
    num_workers: int


@dataclass
class Settings:
    service: ServiceConfig
    model: ModelConfig
    inference: InferenceConfig
    concurrency: ConcurrencyConfig


# ── Hardware Detection ─────────────────────────────────────────────────────────


def _detect_arch() -> str:
    """
    Return the canonical CPU architecture name, normalizing ARM variants to "arm64".

    Returns:
        str: `"arm64"` for ARM architectures (`"arm64"` or `"aarch64"`), otherwise `"x86_64"`.
    """
    arch = platform.machine().lower()
    # Normalize arm64 / aarch64 → arm64
    return "arm64" if arch in ("arm64", "aarch64") else "x86_64"


def _detect_os() -> str:
    """
    Return the current operating system name in lowercase.

    Returns:
        os_name (str): Lowercase OS name as given by the runtime (common values include "linux", "darwin", and "windows").
    """
    return platform.system().lower()  # "linux" | "darwin"


def _cuda_available() -> bool:
    """
    Check whether CUDA is available to ctranslate2.

    Returns:
        `true` if ctranslate2 reports CUDA compute type support, `false` otherwise.
    """
    try:
        import ctranslate2

        return "cuda" in ctranslate2.get_supported_compute_types("cuda")
    except Exception:
        return False


def _physical_core_count() -> int:
    """
    Return the number of physical CPU cores available on the system.

    Attempts to return the physical core count; if that cannot be determined, returns the logical CPU count; if that is unavailable, returns 4.

    Returns:
        int: Number of CPU cores (physical if detectable, otherwise logical or 4).
    """
    try:
        # psutil gives physical (not logical) cores
        import psutil

        return psutil.cpu_count(logical=False) or os.cpu_count() or 4
    except ImportError:
        return os.cpu_count() or 4


def _resolve_device(requested: str) -> Device:
    """
    Selects the runtime device ("cpu" or "cuda") based on the requested preference and system capabilities.

    Parameters:
        requested (str): Use "auto" to detect the best device for the current system; otherwise pass "cpu" or "cuda" to force a device.

    Returns:
        str: The chosen device, either "cuda" or "cpu".

    Notes:
        If running on Apple Silicon (darwin + arm64) and CUDA is unavailable, the function prints an informational note indicating MPS is not yet supported by CTranslate2 and returns "cpu".
    """
    if requested != "auto":
        return requested  # type: ignore

    os_name = _detect_os()
    arch = _detect_arch()

    if _cuda_available():
        return "cuda"

    # Apple Silicon: MPS exists but CTranslate2 doesn't use it yet;
    # cpu+int8 is still the right call, just note it in logs.
    if os_name == "darwin" and arch == "arm64":
        print(
            "[config] Apple Silicon detected — using cpu (MPS not yet supported by CTranslate2)"
        )

    return "cpu"


def _resolve_compute_type(requested: str, device: Device) -> ComputeType:
    """
    Select an appropriate compute type based on the requested preference and target device.

    If `requested` is not "auto", the same value is returned unchanged. When `requested` is "auto":
    - For a CUDA device, prefers `"float16"` if the first GPU reports at least 8 GB of VRAM; uses `"int8_float16"` if VRAM is below 8 GB; falls back to `"float16"` if VRAM cannot be queried.
    - For a CPU device, chooses `"int8"`.

    Parameters:
        requested (str): Desired compute type or `"auto"` to select one automatically.
        device (Device): Target device, either `"cuda"` or `"cpu"`.

    Returns:
        ComputeType: One of `"float16"`, `"int8_float16"`, or `"int8"` according to the rules above.
    """
    if requested != "auto":
        return requested  # type: ignore

    if device == "cuda":
        # Check VRAM to decide float16 vs int8_float16
        try:
            output = subprocess.check_output(
                [
                    "nvidia-smi",
                    "--query-gpu=memory.total",
                    "--format=csv,noheader,nounits",
                ],
                text=True,
            ).strip()
            vram_mb = int(output.split("\n")[0])  # first GPU
            if vram_mb >= 8000:
                return "float16"
            else:
                print(
                    f"[config] VRAM={vram_mb}MB (<8GB) — using int8_float16 to save memory"
                )
                return "int8_float16"
        except Exception:
            return "float16"  # assume enough VRAM if we can't query

    # CPU path
    arch = _detect_arch()
    if arch == "arm64":
        # ARM NEON has fast int8; float32 is unnecessary
        return "int8"
    else:
        # x86 with AVX2/AVX512 — int8 is well-optimized
        return "int8"


def _resolve_cpu_threads(requested: int) -> int:
    """
    Choose the number of CPU threads to use, returning the requested value or a sensible default when auto-selected.

    Parameters:
        requested (int): Number of threads requested; pass 0 to auto-select based on system cores.

    Returns:
        int: The chosen number of CPU threads — `requested` if non-zero, otherwise half the number of physical cores (minimum 1).

    Notes:
        When auto-selecting, the function prints the detected physical core count and the resolved thread count to stdout.
    """
    if requested != 0:
        return requested
    # Use half of physical cores — leaves headroom for diarization pipeline
    cores = _physical_core_count()
    resolved = max(1, cores // 2)
    print(f"[config] Detected {cores} physical cores → using {resolved} cpu_threads")
    return resolved


def _resolve_max_workers(requested: int, device: Device) -> int:
    """
    Selects the maximum number of concurrent workers based on an explicit request and the target device.

    Parameters:
        requested (int): If non-zero, this value is used directly as the max workers.
        device (Device): Target device, either `"cuda"` or `"cpu"`.

    Returns:
        int: The chosen max workers — `requested` if non-zero; otherwise `1` when `device` is `"cuda"`, `2` when `device` is `"cpu"`.
    """
    if requested != 0:
        return requested
    # GPU: no benefit to parallelism (serialized by GPU)
    # CPU: allow 2 concurrent requests (they'll share cpu_threads pool)
    return 1 if device == "cuda" else 2


# ── Env Overrides ──────────────────────────────────────────────────────────────
# Any value in config.toml can be overridden with WHISPER_<SECTION>_<KEY>
# e.g. WHISPER_MODEL_SIZE=medium, WHISPER_SERVICE_SOCKET_PATH=/run/whisper.sock


def _env(section: str, key: str, fallback):
    """
    Builds an environment variable name from section and key, reads it if present, and returns its value cast to the type of fallback.

    Parameters:
        section (str): Section name used to form the environment variable prefix (WHISPER_<SECTION>_<KEY>).
        key (str): Key name used to form the environment variable suffix (WHISPER_<SECTION>_<KEY>).
        fallback: Value whose type determines the cast; returned when the environment variable is not set.

    Returns:
        The environment variable value cast to the type of `fallback` (`bool`, `int`, `float`, or `str`), or `fallback` if the variable is not present.
    """
    env_key = f"WHISPER_{section.upper()}_{key.upper()}"
    val = os.environ.get(env_key)
    if val is None:
        return fallback
    # Cast to the type of the fallback
    if isinstance(fallback, bool):
        return val.lower() in ("1", "true", "yes")
    if isinstance(fallback, int):
        return int(val)
    if isinstance(fallback, float):
        return float(val)
    return val


# ── Loader ─────────────────────────────────────────────────────────────────────


def load_settings(config_path: str | Path = "config.toml") -> Settings:
    """
    Load application settings from a TOML configuration file, applying environment overrides and runtime-detected defaults.

    Environment variables with the pattern WHISPER_<SECTION>_<KEY> override values in the file; device, compute type, and concurrency defaults are resolved automatically based on the host environment.

    Parameters:
        config_path (str | Path): Path to the TOML configuration file. Defaults to "config.toml".

    Returns:
        Settings: A populated Settings dataclass containing service, model, inference, and concurrency configurations.

    Raises:
        FileNotFoundError: If the specified configuration file does not exist.
    """
    path = Path(config_path)
    if not path.exists():
        raise FileNotFoundError(f"Config file not found: {path.resolve()}")

    with open(path, "rb") as f:
        raw = tomllib.load(f)

    svc = raw.get("service", {})
    mdl = raw.get("model", {})
    inf = raw.get("inference", {})
    con = raw.get("concurrency", {})

    # --- Device + compute resolution (detection happens here) ---
    device = _resolve_device(_env("model", "device", mdl.get("device", "auto")))
    compute_type = _resolve_compute_type(
        _env("model", "compute_type", mdl.get("compute_type", "auto")),
        device,
    )
    cpu_threads = _resolve_cpu_threads(
        _env("concurrency", "cpu_threads", con.get("cpu_threads", 0))
    )
    max_workers = _resolve_max_workers(
        _env("concurrency", "max_workers", con.get("max_workers", 0)),
        device,
    )

    settings = Settings(
        service=ServiceConfig(
            socket_path=_env(
                "service", "socket_path", svc.get("socket_path", "/tmp/whisper.sock")
            ),
            log_level=_env("service", "log_level", svc.get("log_level", "info")),
            max_audio_size_mb=_env(
                "service", "max_audio_size_mb", svc.get("max_audio_size_mb", 100)
            ),
        ),
        model=ModelConfig(
            size=_env("model", "size", mdl.get("size", "large-v3")),
            download_dir=_env(
                "model",
                "download_dir",
                mdl.get("download_dir", "/var/lib/whisper/models"),
            ),
            device=device,
            compute_type=compute_type,
        ),
        inference=InferenceConfig(
            beam_size=_env("inference", "beam_size", inf.get("beam_size", 5)),
            vad_filter=_env("inference", "vad_filter", inf.get("vad_filter", True)),
            vad_min_silence_ms=_env(
                "inference", "vad_min_silence_ms", inf.get("vad_min_silence_ms", 500)
            ),
            no_speech_threshold=_env(
                "inference", "no_speech_threshold", inf.get("no_speech_threshold", 0.6)
            ),
            log_prob_threshold=_env(
                "inference", "log_prob_threshold", inf.get("log_prob_threshold", -1.0)
            ),
            compression_ratio_threshold=_env(
                "inference",
                "compression_ratio_threshold",
                inf.get("compression_ratio_threshold", 2.4),
            ),
            word_timestamps=_env(
                "inference", "word_timestamps", inf.get("word_timestamps", True)
            ),
            initial_prompt=_env(
                "inference", "initial_prompt", inf.get("initial_prompt", "")
            ),
        ),
        concurrency=ConcurrencyConfig(
            max_workers=max_workers,
            cpu_threads=cpu_threads,
            num_workers=_env("concurrency", "num_workers", con.get("num_workers", 1)),
        ),
    )

    _print_summary(settings)
    return settings


def _print_summary(s: Settings):
    """
    Prints a concise, formatted runtime summary of the provided Settings.

    The summary includes OS and architecture, selected device and compute type, model size, configured CPU threads and max workers, socket path, and maximum audio size in megabytes. Output is written to standard output.

    Parameters:
        s (Settings): Settings instance to summarize.
    """
    print("─" * 50)
    print(f"  OS/Arch      : {_detect_os()} / {_detect_arch()}")
    print(f"  Device       : {s.model.device}")
    print(f"  Compute type : {s.model.compute_type}")
    print(f"  Model        : {s.model.size}")
    print(f"  CPU threads  : {s.concurrency.cpu_threads}")
    print(f"  Max workers  : {s.concurrency.max_workers}")
    print(f"  Socket       : {s.service.socket_path}")
    print(f"  Max Audio Size: {s.service.max_audio_size_mb}MB")
    print("─" * 50)

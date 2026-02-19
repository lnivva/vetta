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
    Return the normalized CPU architecture name.
    
    Returns:
        str: 'arm64' if the detected machine architecture is ARM (including 'aarch64'), 'x86_64' otherwise.
    """
    arch = platform.machine().lower()
    # Normalize arm64 / aarch64 → arm64
    return "arm64" if arch in ("arm64", "aarch64") else "x86_64"


def _detect_os() -> str:
    """
    Get the current operating system name in lowercase.
    
    Returns:
        The operating system name in lowercase (e.g., "linux", "darwin", "windows").
    """
    return platform.system().lower()  # "linux" | "darwin"


def _cuda_available() -> bool:
    """
    Detect whether a CTranslate2 CUDA backend is available.
    
    Returns:
        `True` if CTranslate2 is importable and reports `cuda` as a supported compute type, `False` otherwise.
    """
    try:
        import ctranslate2
        return "cuda" in ctranslate2.get_supported_compute_types("cuda")
    except Exception:
        return False


def _physical_core_count() -> int:
    """
    Detect the system's number of physical CPU cores, using safe fallbacks.
    
    Attempts to use psutil.cpu_count(logical=False); if that is unavailable or returns None, falls back to os.cpu_count(); if that is also unavailable, returns 4.
    
    Returns:
        int: Detected number of physical CPU cores, or 4 if detection fails.
    """
    try:
        # psutil gives physical (not logical) cores
        import psutil
        return psutil.cpu_count(logical=False) or os.cpu_count() or 4
    except ImportError:
        return os.cpu_count() or 4


def _resolve_device(requested: str) -> Device:
    """
    Choose the device to use ('cuda' or 'cpu') based on the requested preference and system capabilities.
    
    Parameters:
        requested (str): Desired device string; use "auto" to let the function detect the best device.
    
    Returns:
        Device: The chosen device — the original requested value if not "auto", otherwise "cuda" when CUDA is available, or "cpu".
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
        print("[config] Apple Silicon detected — using cpu (MPS not yet supported by CTranslate2)")

    return "cpu"


def _resolve_compute_type(requested: str, device: Device) -> ComputeType:
    """
    Choose the compute type to use for model inference based on the requested preference and the target device.
    
    If `requested` is not "auto", that value is returned. For `device == "cuda"`, the function attempts to query GPU VRAM and selects `"float16"` for GPUs with at least 8000 MB, otherwise `"int8_float16"`; if the VRAM query fails it defaults to `"float16"`. For CPU targets the function selects `"int8"`.
    
    Parameters:
        requested (str): Requested compute type string or "auto" to let the function decide.
        device (Device): Target device, either "cuda" or "cpu".
    
    Returns:
        ComputeType: The chosen compute type: `"float16"`, `"int8_float16"`, or `"int8"`.
    """
    if requested != "auto":
        return requested  # type: ignore

    if device == "cuda":
        # Check VRAM to decide float16 vs int8_float16
        try:
            output = subprocess.check_output(
                ["nvidia-smi", "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
                text=True,
            ).strip()
            vram_mb = int(output.split("\n")[0])  # first GPU
            if vram_mb >= 8000:
                return "float16"
            else:
                print(f"[config] VRAM={vram_mb}MB (<8GB) — using int8_float16 to save memory")
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
    Determine the number of CPU threads to use.
    
    If `requested` is nonzero, that value is returned. Otherwise the function chooses half of the machine's physical cores (minimum 1) as a sensible default. May print a detection message showing the detected physical cores and the chosen thread count.
    
    Parameters:
        requested (int): Requested number of CPU threads; use 0 to auto-resolve.
    
    Returns:
        int: Resolved CPU thread count (at least 1).
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
    Selects the maximum number of concurrent workers based on an explicit request or the target device.
    
    Parameters:
    	requested (int): Requested max workers; use 0 to let the resolver choose a sensible default.
    	device (Device): Target device ("cuda" or "cpu") used when `requested` is 0.
    
    Returns:
    	max_workers (int): The chosen maximum number of concurrent workers (1 for CUDA, 2 for CPU unless overridden by `requested`).
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
    Read a WHISPER_<SECTION>_<KEY> environment variable and return its value, using the provided fallback when the variable is not set.
    
    Parameters:
        section (str): Section name used to build the environment variable (e.g., "model" -> WHISPER_MODEL_<KEY>).
        key (str): Key name used to build the environment variable.
        fallback: Default value returned when the environment variable is not set; also determines the target type for casting.
    
    Returns:
        The environment variable value cast to the type of `fallback`, or `fallback` if the variable is not set. Casting rules:
        - If `fallback` is a bool, returns `True` when the value (case-insensitive) is "1", "true", or "yes"; otherwise `False`.
        - If `fallback` is an int, returns `int(value)`.
        - If `fallback` is a float, returns `float(value)`.
        - Otherwise returns the raw string value.
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
    Load configuration from a TOML file, apply environment-variable overrides, resolve device/compute and concurrency defaults based on the host, and return a populated Settings object.
    
    Parameters:
        config_path (str | Path): Path to the TOML configuration file (defaults to "config.toml"). Environment variables of the form WHISPER_<SECTION>_<KEY> override values from the file.
    
    Returns:
        Settings: Aggregated settings containing service, model, inference, and concurrency configurations. The model's device and compute_type may be auto-resolved based on system hardware and available libraries.
    
    Raises:
        FileNotFoundError: If the provided config_path does not exist.
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
    device = _resolve_device(
        _env("model", "device", mdl.get("device", "auto"))
    )
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
            socket_path=_env("service", "socket_path", svc.get("socket_path", "/tmp/whisper.sock")),
            log_level=_env("service", "log_level", svc.get("log_level", "info")),
        ),
        model=ModelConfig(
            size=_env("model", "size", mdl.get("size", "large-v3")),
            download_dir=_env("model", "download_dir", mdl.get("download_dir", "/var/lib/whisper/models")),
            device=device,
            compute_type=compute_type,
        ),
        inference=InferenceConfig(
            beam_size=_env("inference", "beam_size", inf.get("beam_size", 5)),
            vad_filter=_env("inference", "vad_filter", inf.get("vad_filter", True)),
            vad_min_silence_ms=_env("inference", "vad_min_silence_ms", inf.get("vad_min_silence_ms", 500)),
            no_speech_threshold=_env("inference", "no_speech_threshold", inf.get("no_speech_threshold", 0.6)),
            log_prob_threshold=_env("inference", "log_prob_threshold", inf.get("log_prob_threshold", -1.0)),
            compression_ratio_threshold=_env("inference", "compression_ratio_threshold",
                                             inf.get("compression_ratio_threshold", 2.4)),
            word_timestamps=_env("inference", "word_timestamps", inf.get("word_timestamps", True)),
            initial_prompt=_env("inference", "initial_prompt", inf.get("initial_prompt", "")),
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
    Print a concise runtime summary of the detected system and resolved settings.
    
    Includes OS/architecture, selected device and compute type, model size, CPU threads, max workers, and socket path.
    
    Parameters:
        s (Settings): Settings instance whose values are displayed.
    """
    print("─" * 50)
    print(f"  OS/Arch      : {_detect_os()} / {_detect_arch()}")
    print(f"  Device       : {s.model.device}")
    print(f"  Compute type : {s.model.compute_type}")
    print(f"  Model        : {s.model.size}")
    print(f"  CPU threads  : {s.concurrency.cpu_threads}")
    print(f"  Max workers  : {s.concurrency.max_workers}")
    print(f"  Socket       : {s.service.socket_path}")
    print("─" * 50)
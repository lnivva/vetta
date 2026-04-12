import os
import platform
import subprocess
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal, overload

Device = Literal["cuda", "cpu"]
ComputeType = Literal["float16", "int8_float16", "int8", "float32"]


@dataclass
class ServiceConfig:
    address: str
    log_level: str
    max_audio_size_mb: int

    @property
    def is_unix_socket(self) -> bool:
        return self.address.startswith("unix://")

    @property
    def socket_path(self) -> str | None:
        """Return the filesystem path if this is a UDS address, else None."""
        if self.is_unix_socket:
            return self.address[len("unix://") :]
        return None


@dataclass
class ModelConfig:
    size: str
    download_dir: str
    device: Device
    compute_type: ComputeType
    hf_token: str = field(repr=False)


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
class DiarizationConfig:
    """Configuration for the optional pyannote speaker-diarization pipeline."""

    enabled: bool
    model: str
    device: Device
    min_speakers: int  # 0 = auto
    max_speakers: int  # 0 = auto


@dataclass
class EmbeddingsConfig:
    """Configuration for the text embeddings provider (Voyage AI)."""

    api_key: str = field(repr=False)


@dataclass
class Settings:
    service: ServiceConfig
    model: ModelConfig
    inference: InferenceConfig
    concurrency: ConcurrencyConfig
    diarization: DiarizationConfig
    embeddings: EmbeddingsConfig


def _detect_arch() -> str:
    """
    Return the canonical CPU technical name, normalizing ARM variants to "arm64".

    Returns:
        str: `"arm64"` for ARM architectures (`"arm64"` or `"aarch64"`), otherwise `"x86_64"`.
    """
    arch = platform.machine().lower()
    return "arm64" if arch in ("arm64", "aarch64") else "x86_64"


def _detect_os() -> str:
    """
    Return the current operating system name in lowercase.

    Returns:
        os_name (str): Lowercase OS name as given by the runtime.
    """
    return platform.system().lower()


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

    Attempts to return the physical core count; if that cannot be determined,
    returns the logical CPU count; if that is unavailable, returns 4.

    Returns:
        int: Number of CPU cores (physical if detectable, otherwise logical or 4).
    """
    try:
        import psutil

        return psutil.cpu_count(logical=False) or os.cpu_count() or 4
    except ImportError:
        return os.cpu_count() or 4


def _resolve_device(requested: str) -> Device:
    """
    Selects the runtime device ("cpu" or "cuda") based on the requested
    preference and system capabilities.

    Parameters:
        requested (str): Use "auto" to detect the best device; otherwise
                         pass "cpu" or "cuda" to force a device.

    Returns:
        str: The chosen device, either "cuda" or "cpu".
    """
    if requested != "auto":
        return requested  # type: ignore

    os_name = _detect_os()
    arch = _detect_arch()

    if _cuda_available():
        return "cuda"

    if os_name == "darwin" and arch == "arm64":
        print(
            "[config] Apple Silicon detected — using cpu "
            "(MPS not yet supported by CTranslate2)"
        )

    return "cpu"


def _resolve_compute_type(requested: str, device: Device) -> ComputeType:
    """
    Select an appropriate compute type based on the requested preference
    and target device.

    Parameters:
        requested (str): Desired compute type or `"auto"` to select one automatically.
        device (Device): Target device, either `"cuda"` or `"cpu"`.

    Returns:
        ComputeType: The resolved compute type.
    """
    if requested != "auto":
        return requested  # type: ignore

    if device == "cuda":
        try:
            output = subprocess.check_output(
                [
                    "nvidia-smi",
                    "--query-gpu=memory.total",
                    "--format=csv,noheader,nounits",
                ],
                text=True,
            ).strip()
            vram_mb = int(output.split("\n")[0])
            if vram_mb >= 8000:
                return "float16"
            else:
                print(
                    f"[config] VRAM={vram_mb}MB (<8GB) — using int8_float16 to save memory"
                )
                return "int8_float16"
        except Exception:
            return "float16"

    return "int8"


def _resolve_cpu_threads(requested: int) -> int:
    """
    Choose the number of CPU threads to use.

    Parameters:
        requested (int): Number of threads requested; pass 0 to auto-select.

    Returns:
        int: The chosen number of CPU threads.
    """
    if requested != 0:
        return requested
    cores = _physical_core_count()
    resolved = max(1, cores // 2)
    print(f"[config] Detected {cores} physical cores → using {resolved} cpu_threads")
    return resolved


def _resolve_max_workers(requested: int, device: Device) -> int:
    """
    Selects the maximum number of concurrent workers.

    Parameters:
        requested (int): If non-zero, used directly.
        device (Device): Target device, either `"cuda"` or `"cpu"`.

    Returns:
        int: The chosen max workers.
    """
    if requested != 0:
        return requested
    return 1 if device == "cuda" else 2


@overload
def _env(section: str, key: str, fallback: bool) -> bool: ...


@overload
def _env(section: str, key: str, fallback: int) -> int: ...


@overload
def _env(section: str, key: str, fallback: float) -> float: ...


@overload
def _env(section: str, key: str, fallback: str) -> str: ...


def _env(
    section: str, key: str, fallback: str | bool | int | float
) -> str | bool | int | float:
    """
    Read WHISPER_<SECTION>_<KEY> from environment, cast to type of fallback.

    Parameters:
        section (str): Section name for the env var prefix.
        key (str): Key name for the env var suffix.
        fallback: Default value whose type determines the cast.

    Returns:
        The environment variable value cast appropriately, or fallback.
    """
    env_key = f"WHISPER_{section.upper()}_{key.upper()}"
    val = os.environ.get(env_key)
    if val is None:
        return fallback
    if isinstance(fallback, bool):
        return val.lower() in ("1", "true", "yes")
    if isinstance(fallback, int):
        return int(val)
    if isinstance(fallback, float):
        return float(val)
    return val


def load_settings(config_path: str | Path = "config.toml") -> Settings:
    """
    Load application settings from a TOML configuration file, applying
    environment overrides and runtime-detected defaults.

    Parameters:
        config_path (str | Path): Path to the TOML configuration file.

    Returns:
        Settings: A populated Settings dataclass.

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
    dia = raw.get("diarization", {})
    emb = raw.get("embeddings", {})

    # --- Device + compute resolution ---
    device = _resolve_device(_env("model", "device", str(mdl.get("device", "auto"))))
    compute_type = _resolve_compute_type(
        _env("model", "compute_type", str(mdl.get("compute_type", "auto"))),
        device,
    )
    cpu_threads = _resolve_cpu_threads(
        _env("concurrency", "cpu_threads", int(con.get("cpu_threads", 0)))
    )
    max_workers = _resolve_max_workers(
        _env("concurrency", "max_workers", int(con.get("max_workers", 0))),
        device,
    )

    # --- Diarization device resolution ---
    dia_device_raw = _env("diarization", "device", str(dia.get("device", "auto")))
    if dia_device_raw == "auto":
        dia_device = device
    else:
        dia_device = _resolve_device(dia_device_raw)

    raw_address = svc.get("address")
    if raw_address is None and "socket_path" in svc:
        raw_address = f"unix://{svc['socket_path']}"

    legacy_socket_path = os.environ.get("WHISPER_SERVICE_SOCKET_PATH")
    if "WHISPER_SERVICE_ADDRESS" in os.environ:
        resolved_address = _env(
            "service",
            "address",
            raw_address or "unix:///tmp/whisper.sock",
        )
    elif legacy_socket_path:
        resolved_address = f"unix://{legacy_socket_path}"
    else:
        resolved_address = raw_address or "unix:///tmp/whisper.sock"

    settings = Settings(
        service=ServiceConfig(
            address=resolved_address,
            log_level=_env("service", "log_level", str(svc.get("log_level", "info"))),
            max_audio_size_mb=_env(
                "service", "max_audio_size_mb", int(svc.get("max_audio_size_mb", 100))
            ),
        ),
        model=ModelConfig(
            size=_env("model", "size", str(mdl.get("size", "large-v3"))),
            download_dir=_env(
                "model",
                "download_dir",
                str(mdl.get("download_dir", "/var/lib/whisper/models")),
            ),
            device=device,
            compute_type=compute_type,
            hf_token=_env("model", "hf_token", str(mdl.get("hf_token", ""))),
        ),
        inference=InferenceConfig(
            beam_size=_env("inference", "beam_size", int(inf.get("beam_size", 5))),
            vad_filter=_env(
                "inference", "vad_filter", bool(inf.get("vad_filter", True))
            ),
            vad_min_silence_ms=_env(
                "inference",
                "vad_min_silence_ms",
                int(inf.get("vad_min_silence_ms", 300)),
            ),
            no_speech_threshold=_env(
                "inference",
                "no_speech_threshold",
                float(inf.get("no_speech_threshold", 0.6)),
            ),
            log_prob_threshold=_env(
                "inference",
                "log_prob_threshold",
                float(inf.get("log_prob_threshold", -0.5)),
            ),
            compression_ratio_threshold=_env(
                "inference",
                "compression_ratio_threshold",
                float(inf.get("compression_ratio_threshold", 2.0)),
            ),
            word_timestamps=_env(
                "inference", "word_timestamps", bool(inf.get("word_timestamps", True))
            ),
            initial_prompt=_env(
                "inference", "initial_prompt", str(inf.get("initial_prompt", ""))
            ),
        ),
        concurrency=ConcurrencyConfig(
            max_workers=max_workers,
            cpu_threads=cpu_threads,
            num_workers=_env(
                "concurrency", "num_workers", int(con.get("num_workers", 1))
            ),
        ),
        diarization=DiarizationConfig(
            enabled=_env("diarization", "enabled", bool(dia.get("enabled", True))),
            model=_env(
                "diarization",
                "model",
                str(dia.get("model", "pyannote/speaker-diarization-3.1")),
            ),
            device=dia_device,
            min_speakers=_env(
                "diarization", "min_speakers", int(dia.get("min_speakers", 0))
            ),
            max_speakers=_env(
                "diarization", "max_speakers", int(dia.get("max_speakers", 0))
            ),
        ),
        embeddings=EmbeddingsConfig(
            api_key=_env("embeddings", "api_key", str(emb.get("api_key", "")))
        ),
    )

    hf_token = settings.model.hf_token
    if hf_token:
        os.environ["HF_TOKEN"] = hf_token
        try:
            from huggingface_hub import login

            login(token=hf_token)
            print("[config] Successfully logged into Hugging Face Hub.")
        except ImportError:
            print(
                "[config] huggingface_hub not installed; relying on HF_TOKEN env var."
            )
        except Exception as exc:
            print(
                f"[config] Hugging Face login failed ({exc}); "
                f"falling back to HF_TOKEN env var."
            )

    _print_summary(settings)
    return settings


def _print_summary(s: Settings):
    """Print a concise runtime summary of the provided Settings."""
    print("─" * 50)
    print(f"  OS/Arch        : {_detect_os()} / {_detect_arch()}")
    print(f"  Device         : {s.model.device}")
    print(f"  Compute type   : {s.model.compute_type}")
    print(f"  Model          : {s.model.size}")
    print(f"  HF Token       : {'<configured>' if s.model.hf_token else '<missing>'}")
    print(
        f"  Emb API Key    : {'<configured>' if s.embeddings.api_key else '<missing>'}"
    )
    print(f"  CPU threads    : {s.concurrency.cpu_threads}")
    print(f"  Max workers    : {s.concurrency.max_workers}")
    print(f"  Address        : {s.service.address}")
    print(f"  Max Audio Size : {s.service.max_audio_size_mb}MB")
    print(f"  Diarization    : {'enabled' if s.diarization.enabled else 'disabled'}")
    if s.diarization.enabled:
        print(f"    Model        : {s.diarization.model}")
        print(f"    Device       : {s.diarization.device}")
    print("─" * 50)

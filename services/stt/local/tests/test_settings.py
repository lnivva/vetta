import shutil
from pathlib import Path
from unittest.mock import patch

import pytest

from settings import (
    ServiceConfig,
    load_settings,
    _detect_arch,
    _resolve_device,
    _resolve_compute_type,
    _resolve_cpu_threads,
    _resolve_max_workers,
)

FIXTURES_DIR = Path(__file__).parent / "fixtures"


@pytest.fixture
def minimal_config(tmp_path):
    """Copy the minimal config fixture to a temp directory and return its path."""
    src = FIXTURES_DIR / "minimal_config.toml"
    dst = tmp_path / "config.toml"
    shutil.copy(src, dst)
    return dst


@pytest.fixture
def tcp_config(tmp_path):
    src = FIXTURES_DIR / "tcp_config.toml"
    dst = tmp_path / "config.toml"
    shutil.copy(src, dst)
    return dst


class TestServiceConfig:
    """Verify the is_unix_socket / socket_path property logic."""

    def test_unix_address_detected_as_socket(self):
        cfg = ServiceConfig(
            address="unix:///tmp/whisper.sock",
            log_level="info",
            max_audio_size_mb=100,
        )
        assert cfg.is_unix_socket is True
        assert cfg.socket_path == "/tmp/whisper.sock"

    def test_tcp_address_not_detected_as_socket(self):
        cfg = ServiceConfig(
            address="0.0.0.0:50051",
            log_level="info",
            max_audio_size_mb=100,
        )
        assert cfg.is_unix_socket is False
        assert cfg.socket_path is None

    def test_unix_relative_path(self):
        cfg = ServiceConfig(
            address="unix://relative/path.sock",
            log_level="info",
            max_audio_size_mb=100,
        )
        assert cfg.is_unix_socket is True
        assert cfg.socket_path == "relative/path.sock"

    def test_localhost_tcp_not_confused_with_unix(self):
        cfg = ServiceConfig(
            address="localhost:50051",
            log_level="info",
            max_audio_size_mb=100,
        )
        assert cfg.is_unix_socket is False
        assert cfg.socket_path is None

    # ── Config Loading ────────────────────────────────────────────


class TestConfigLoading:
    def test_loads_valid_config_model(self, minimal_config):
        s = load_settings(minimal_config)
        assert s.model.size == "small"
        assert s.model.device == "cpu"
        assert s.model.compute_type == "int8"

    def test_loads_valid_config_inference(self, minimal_config):
        s = load_settings(minimal_config)
        assert s.inference.beam_size == 3
        assert s.inference.vad_filter is True
        assert s.inference.word_timestamps is True

    def test_loads_valid_config_concurrency(self, minimal_config):
        s = load_settings(minimal_config)
        assert s.concurrency.cpu_threads == 2
        assert s.concurrency.max_workers == 1

    def test_loads_valid_config_address(self, minimal_config):
        s = load_settings(minimal_config)
        assert s.service.address == "unix:///tmp/test-whisper.sock"
        assert s.service.is_unix_socket is True
        assert s.service.socket_path == "/tmp/test-whisper.sock"

    def test_loads_tcp_config(self, tcp_config):
        s = load_settings(tcp_config)
        assert s.service.address == "0.0.0.0:50051"
        assert s.service.is_unix_socket is False
        assert s.service.socket_path is None

    def test_missing_config_raises(self, tmp_path):
        with pytest.raises(FileNotFoundError, match="Config file not found"):
            load_settings(tmp_path / "nonexistent.toml")

    def test_diarization_defaults_to_disabled(self, minimal_config):
        s = load_settings(minimal_config)
        assert s.diarization.enabled is False
        assert s.diarization.hf_token == ""

    # ── Env Overrides ─────────────────────────────────────────────


class TestEnvOverrides:
    """Verify that WHISPER_<SECTION>_<KEY> env vars take precedence over TOML."""

    def test_env_overrides_model_size(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_MODEL_SIZE", "medium")
        s = load_settings(minimal_config)
        # TOML says "small", env says "medium" — env must win
        assert s.model.size == "medium"

    def test_env_overrides_address(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_SERVICE_ADDRESS", "0.0.0.0:9999")
        s = load_settings(minimal_config)
        # TOML says unix socket, env says TCP — env must win
        assert s.service.address == "0.0.0.0:9999"
        assert s.service.is_unix_socket is False

    def test_env_overrides_bool_false(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_INFERENCE_VAD_FILTER", "false")
        s = load_settings(minimal_config)
        # TOML says true, env says false — env must win
        assert s.inference.vad_filter is False

    def test_env_overrides_bool_true(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_INFERENCE_VAD_FILTER", "true")
        s = load_settings(minimal_config)
        assert s.inference.vad_filter is True

    def test_env_overrides_float(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_INFERENCE_NO_SPEECH_THRESHOLD", "0.9")
        s = load_settings(minimal_config)
        # TOML says 0.6, env says 0.9 — env must win
        assert s.inference.no_speech_threshold == pytest.approx(0.9)
        assert s.inference.no_speech_threshold != pytest.approx(0.6)

    def test_env_overrides_int(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_CONCURRENCY_CPU_THREADS", "16")
        s = load_settings(minimal_config)
        # TOML says 2, env says 16 — env must win
        assert s.concurrency.cpu_threads == 16
        assert s.concurrency.cpu_threads != 2

    def test_env_override_does_not_affect_other_fields(
        self, minimal_config, monkeypatch
    ):
        """Changing one field must not bleed into unrelated fields."""
        monkeypatch.setenv("WHISPER_MODEL_SIZE", "tiny")
        s = load_settings(minimal_config)
        assert s.model.size == "tiny"
        # Everything else should remain at TOML values
        assert s.model.device == "cpu"
        assert s.inference.beam_size == 3
        assert s.concurrency.cpu_threads == 2

    # ── Hardware Detection ────────────────────────────────────────


class TestDeviceResolution:
    def test_explicit_cpu_passes_through(self):
        result = _resolve_device("cpu")
        assert result == "cpu"

    def test_explicit_cuda_passes_through(self):
        """Explicit 'cuda' is returned even if CUDA isn't available."""
        result = _resolve_device("cuda")
        assert result == "cuda"

    def test_auto_selects_cuda_when_available(self):
        with patch("settings._cuda_available", return_value=True):
            assert _resolve_device("auto") == "cuda"

    def test_auto_falls_back_to_cpu_when_no_cuda(self):
        with patch("settings._cuda_available", return_value=False):
            assert _resolve_device("auto") == "cpu"

    def test_auto_returns_cpu_on_apple_silicon(self):
        """Apple Silicon should get cpu since CTranslate2 doesn't support MPS."""
        with (
            patch("settings._cuda_available", return_value=False),
            patch("settings._detect_os", return_value="darwin"),
            patch("settings._detect_arch", return_value="arm64"),
        ):
            assert _resolve_device("auto") == "cpu"


class TestComputeTypeResolution:
    def test_explicit_values_pass_through(self):
        assert _resolve_compute_type("float16", "cuda") == "float16"
        assert _resolve_compute_type("int8", "cpu") == "int8"
        assert _resolve_compute_type("int8_float16", "cuda") == "int8_float16"
        assert _resolve_compute_type("float32", "cpu") == "float32"

    def test_auto_cuda_high_vram_selects_float16(self):
        with patch("settings.subprocess.check_output", return_value="16000\n"):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "float16"

    def test_auto_cuda_exactly_8gb_selects_float16(self):
        """Boundary: 8000 MB should qualify for float16."""
        with patch("settings.subprocess.check_output", return_value="8000\n"):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "float16"

    def test_auto_cuda_low_vram_selects_int8_float16(self):
        with patch("settings.subprocess.check_output", return_value="4000\n"):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "int8_float16"

    def test_auto_cuda_nvidia_smi_failure_defaults_to_float16(self):
        with patch(
            "settings.subprocess.check_output",
            side_effect=FileNotFoundError("nvidia-smi not found"),
        ):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "float16"

    def test_auto_cpu_always_selects_int8(self):
        """CPU auto should return int8 regardless of architecture."""
        result = _resolve_compute_type("auto", "cpu")
        assert result == "int8"


class TestCpuThreadResolution:
    def test_explicit_value_passes_through(self):
        assert _resolve_cpu_threads(4) == 4
        assert _resolve_cpu_threads(1) == 1

    def test_auto_uses_half_physical_cores(self):
        with patch("settings._physical_core_count", return_value=8):
            assert _resolve_cpu_threads(0) == 4

    def test_auto_floors_at_one(self):
        with patch("settings._physical_core_count", return_value=1):
            result = _resolve_cpu_threads(0)
            assert result >= 1

    def test_auto_with_odd_core_count(self):
        """7 cores → 3 threads (integer division)."""
        with patch("settings._physical_core_count", return_value=7):
            assert _resolve_cpu_threads(0) == 3


class TestMaxWorkersResolution:
    def test_explicit_value_passes_through(self):
        assert _resolve_max_workers(4, "cpu") == 4
        assert _resolve_max_workers(4, "cuda") == 4

    def test_auto_cuda_defaults_to_one(self):
        """GPU inference is typically single-worker to avoid OOM."""
        assert _resolve_max_workers(0, "cuda") == 1

    def test_auto_cpu_defaults_to_two(self):
        assert _resolve_max_workers(0, "cpu") == 2


class TestArchDetection:
    @pytest.mark.parametrize(
        "raw, expected",
        [
            ("arm64", "arm64"),
            ("aarch64", "arm64"),
            ("x86_64", "x86_64"),
            ("AMD64", "x86_64"),
        ],
    )
    def test_normalizes_architecture_names(self, raw, expected):
        with patch("platform.machine", return_value=raw):
            assert _detect_arch() == expected

    def test_unknown_arch_falls_through_to_x86(self):
        """Any unrecognized arch string should default to x86_64."""
        with patch("platform.machine", return_value="riscv64"):
            assert _detect_arch() == "x86_64"

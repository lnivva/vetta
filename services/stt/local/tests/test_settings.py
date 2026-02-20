from unittest.mock import patch

import pytest

from settings import (
    load_settings,
    _detect_arch,
    _resolve_device,
    _resolve_compute_type,
    _resolve_cpu_threads,
)


class TestConfigLoading:
    def test_loads_valid_config(self, minimal_config):
        s = load_settings(minimal_config)
        assert s.model.size == "small"
        assert s.model.device == "cpu"
        assert s.model.compute_type == "int8"
        assert s.inference.beam_size == 3
        assert s.concurrency.cpu_threads == 2

    def test_missing_config_raises(self, tmp_path):
        with pytest.raises(FileNotFoundError):
            load_settings(tmp_path / "nonexistent.toml")

    def test_env_overrides_model_size(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_MODEL_SIZE", "medium")
        s = load_settings(minimal_config)
        assert s.model.size == "medium"

    def test_env_overrides_socket_path(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_SERVICE_SOCKET_PATH", "/run/custom.sock")
        s = load_settings(minimal_config)
        assert s.service.socket_path == "/run/custom.sock"

    def test_env_overrides_bool(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_INFERENCE_VAD_FILTER", "false")
        s = load_settings(minimal_config)
        assert s.inference.vad_filter is False

    def test_env_overrides_float(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_INFERENCE_NO_SPEECH_THRESHOLD", "0.9")
        s = load_settings(minimal_config)
        assert s.inference.no_speech_threshold == pytest.approx(0.9)

    def test_env_overrides_int(self, minimal_config, monkeypatch):
        monkeypatch.setenv("WHISPER_CONCURRENCY_CPU_THREADS", "8")
        s = load_settings(minimal_config)
        assert s.concurrency.cpu_threads == 8


class TestHardwareDetection:
    def test_resolve_device_explicit_cpu(self):
        assert _resolve_device("cpu") == "cpu"

    def test_resolve_device_explicit_cuda(self):
        assert _resolve_device("cuda") == "cuda"

    def test_resolve_device_auto_no_cuda(self):
        with patch("settings._cuda_available", return_value=False):
            assert _resolve_device("auto") == "cpu"

    def test_resolve_device_auto_with_cuda(self):
        with patch("settings._cuda_available", return_value=True):
            assert _resolve_device("auto") == "cuda"

    def test_resolve_compute_explicit(self):
        assert _resolve_compute_type("float16", "cuda") == "float16"
        assert _resolve_compute_type("int8", "cpu") == "int8"

    def test_resolve_compute_auto_cuda_high_vram(self):
        with patch("settings.subprocess.check_output", return_value="16000\n"):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "float16"

    def test_resolve_compute_auto_cuda_low_vram(self):
        with patch("settings.subprocess.check_output", return_value="6000\n"):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "int8_float16"

    def test_resolve_compute_auto_cuda_nvidia_smi_fails(self):
        # If nvidia-smi is not available, should default to float16 safely
        with patch(
            "settings.subprocess.check_output", side_effect=Exception("not found")
        ):
            result = _resolve_compute_type("auto", "cuda")
        assert result == "float16"

    def test_resolve_compute_auto_cpu_arm(self):
        with patch("settings._detect_arch", return_value="arm64"):
            result = _resolve_compute_type("auto", "cpu")
        assert result == "int8"

    def test_resolve_compute_auto_cpu_x86(self):
        with patch("settings._detect_arch", return_value="x86_64"):
            result = _resolve_compute_type("auto", "cpu")
        assert result == "int8"

    def test_cpu_threads_explicit(self):
        assert _resolve_cpu_threads(4) == 4

    def test_cpu_threads_auto(self):
        with patch("settings._physical_core_count", return_value=8):
            assert _resolve_cpu_threads(0) == 4  # half of physical

    def test_cpu_threads_auto_single_core(self):
        with patch("settings._physical_core_count", return_value=1):
            assert _resolve_cpu_threads(0) == 1  # floor at 1

    @pytest.mark.parametrize(
        "raw,expected",
        [
            ("arm64", "arm64"),
            ("aarch64", "arm64"),
            ("x86_64", "x86_64"),
            ("AMD64", "x86_64"),
        ],
    )
    def test_arch_normalization(self, raw, expected):
        with patch("platform.machine", return_value=raw):
            assert _detect_arch() == expected

# ============================================================
# Stage 1: Build ctranslate2 from source with CUDA for aarch64
# ============================================================
FROM nvidia/cuda:12.6.3-cudnn-devel-ubuntu24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    python3.12 python3.12-venv python3.12-dev python3-pip \
    cmake build-essential git curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

# Build CTranslate2 C++ library
RUN git clone --recursive --branch v4.7.1 https://github.com/OpenNMT/CTranslate2.git /tmp/ct2 && \
    cd /tmp/ct2 && mkdir build && cd build && \
    cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DWITH_CUDA=ON \
      -DWITH_CUDNN=ON \
      -DWITH_MKL=OFF \
      -DCUDA_ARCH_LIST="7.5" \
      -DOPENMP_RUNTIME=COMP \
      -DCMAKE_INSTALL_PREFIX=/usr/local && \
    make -j$(nproc) && \
    make install && \
    ldconfig

# Install project deps into venv (WITHOUT ctranslate2)
WORKDIR /app
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-install-project --python python3.12 --no-install-package ctranslate2

# Install CUDA-compiled ctranslate2 into the venv using the venv's own pip
RUN /app/.venv/bin/pip install --no-build-isolation /tmp/ct2/python

# Verify CUDA support is present
RUN /app/.venv/bin/python3.12 -c "import ctranslate2; print('CUDA supported:', 'cuda' in ctranslate2.get_supported_compute_types('cuda'))"

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM nvidia/cuda:12.6.3-cudnn-runtime-ubuntu24.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

RUN apt-get update && apt-get install -y \
    python3.12 python3.12-venv ffmpeg libgomp1 \
    && rm -rf /var/lib/apt/lists/*

# Copy the CUDA-compiled CTranslate2 shared libraries
COPY --from=builder /usr/local/lib/libctranslate2* /usr/local/lib/
RUN ldconfig

# Copy the entire venv (which now contains CUDA-compiled ctranslate2 Python package)
COPY --from=builder /app/.venv /app/.venv

WORKDIR /app
COPY . .

ENV PATH="/app/.venv/bin:$PATH"
ENV LD_LIBRARY_PATH="/usr/local/lib:${LD_LIBRARY_PATH}"
ENV VIRTUAL_ENV="/app/.venv"
ENV CUDA_VISIBLE_DEVICES=0
ENV PYTORCH_CUDA_ALLOC_CONF=expandable_segments:True
ENV OMP_NUM_THREADS=4
ENV MKL_NUM_THREADS=4
ENV CT2_CUDA_ALLOW_FP16=1

EXPOSE 50051

HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 docker system prune -a --volumes -f

CMD python3.12 -c "import grpc; ch = grpc.insecure_channel('unix:///tmp/whisper.sock'); grpc.channel_ready_future(ch).result(timeout=2)" || exit 1

ENTRYPOINT ["python3.12", "main.py", "--config", "config.toml"]

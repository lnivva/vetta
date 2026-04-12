# Configuration

Vetta is composed of multiple services, each with its own configuration file. Every service follows the same
conventions:

- **TOML config files**: each service reads a `config.toml` at startup
- **Environment variable overrides**: any config value can be overridden with a `<PREFIX>_<SECTION>_<KEY>` environment
  variable
- **Auto-detection**: hardware-dependent settings (device, compute type, thread counts) default to `auto` and resolve
  at startup

## Services

| Service                                                 | Config File   | Env Prefix | Language | Description                                                    |
|---------------------------------------------------------|---------------|------------|----------|----------------------------------------------------------------|
| [STT Service](/configuration/stt-service)               | `config.toml` | `WHISPER_` | Python   | Speech-to-text transcription with optional speaker diarization |
| [Embeddings Service](/configuration/embeddings-service) | `config.toml` | `WHISPER_` | Python   | Text vector embeddings via Voyage AI                           |

:::note Shared process

The STT Service and Embeddings Service currently run within the same gRPC server process and share a single
`config.toml` file. They are documented separately because they serve distinct functions and can be configured
independently.

:::
# Embeddings Service Configuration Reference

The Embeddings service provides a gRPC API for generating text vector embeddings powered
by [Voyage AI](https://www.voyageai.com/). It runs alongside the STT service within the same gRPC server process and
shares the global `config.toml` file.

## `[embeddings]`

| Property  | Type     | Default | Description                                                                                            |
|-----------|----------|---------|--------------------------------------------------------------------------------------------------------|
| `api_key` | `string` | `""`    | Voyage AI API key. **Required** for the Embeddings service to function. Requests fail if not provided. |

:::danger

Never commit your `api_key` to version control. Use environment variables or a secrets manager in production.

```bash
export WHISPER_EMBEDDINGS_API_KEY="pa-..."
```

:::

:::warning

If no API key is configured, the service starts but logs a warning. All embedding requests will fail with an `INTERNAL`
gRPC error when the Voyage AI client attempts to authenticate.

:::

### Obtaining an API key

1. **Create an account** at [dash.voyageai.com](https://dash.voyageai.com)
2. **Generate an API key** from the dashboard
3. Set it in `config.toml` or via environment variable:

```toml
[embeddings]
api_key = ""  # use WHISPER_EMBEDDINGS_API_KEY env var
```

## Environment Variable Reference

| Environment Variable         | Config Equivalent      | Type     |
|------------------------------|------------------------|----------|
| `WHISPER_EMBEDDINGS_API_KEY` | `[embeddings] api_key` | `string` |

## Example Configuration

```toml
[embeddings]
api_key = ""  # use WHISPER_EMBEDDINGS_API_KEY env var
```
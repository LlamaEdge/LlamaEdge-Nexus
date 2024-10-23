# Llama-Proxy-Server

Llama-proxy-server serves as a proxy server for LLM APIs.

> [!NOTE]
> The project is still under active development. The existing features still need to be improved and more features will be added in the future.

## Usage

### Start llama-proxy-server

- Download llama-proxy-server

  ```bash
  curl -LO https://github.com/LlamaEdge/llama-proxy-server/releases/latest/download/llama-proxy-server.wasm
  ```

- Start llama-proxy-server

  ```bash
  wasmedge llama-proxy-server.wasm --port 10086
  ```

> `llama-proxy-server` will use `8080` port by default. You can change the port by adding `--port <port>`.

### Register, unregister, list downstream servers

- Register a downstream server

  ```bash
  curl -X POST http://localhost:8080/admin/register/{type} -d "http://localhost:8080"
  ```

  The `{type}` can be `chat`, `whisper`, `image`.

  For example, register a whisper server:

  ```bash
  curl -X POST http://localhost:8080/admin/register/whisper -d "http://localhost:12306"
  ```

- Unregister a downstream server

  ```bash
  curl -X POST http://localhost:8080/admin/unregister/{type} -d "http://localhost:8080"
  ```

  The `{type}` can be `chat`, `whisper`, `image`.

  For example, unregister a whisper server:

  ```bash
  curl -X POST http://localhost:8080/admin/unregister/whisper -d "http://localhost:12306"
  ```

- List available downstream servers

  To list all the registered downstream servers and their types, you can use the following command:

  ```bash
  curl -X POST http://localhost:8080/admin/servers
  ```

  If the downstream servers are registered, the response will be like:

  ```json
  {
    "chat": [],
    "image": [],
    "whisper": [
        "http://0.0.0.0:12306/"
    ]
  }
  ```

### Business endpoints

Currently, `llama-proxy-server` supports the following three types of business endpoints:

- `chat` endpoints (corresponds to [`llama-api-server`](https://github.com/LlamaEdge/LlamaEdge))
  - `/v1/chat/completions`
  - `/v1/completions`
  - `/v1/models`
  - `/v1/embeddings`
  - `/v1/files`
  - `/v1/chunks`
- `whisper` endpoints (corresponds to [`whisper-api-server`](https://github.com/LlamaEdge/whisper-api-server))
  - `/v1/audio/transcriptions`
  - `/v1/audio/translations`
- `image` endpoints (corresponds to [`sd-api-server`](https://github.com/LlamaEdge/sd-api-server))
  - `/v1/images/generations`
  - `/v1/images/edits`

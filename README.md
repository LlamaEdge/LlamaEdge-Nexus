# LlamaEdge-Nexus

LlamaEdge-Nexus is a gateway service for managing and orchestrating LlamaEdge API servers. It provides a unified interface to various AI services including chat completions, audio processing, image generation, and text-to-speech capabilities. Compatible with OpenAI API, LlamaEdge-Nexus allows you to use familiar API formats while working with open-source models. With LlamaEdge-Nexus, you can easily register and manage multiple API servers, handle requests, and monitor the health of your AI services.
<!--
## Installation

- Download LlamaEdge-Nexus binary

  The LlamaEdge-Nexus binaries can be found at the [release page](https://github.com/llamaedge/llamaedge-nexus/releases). To download the binary, you can use the following command:

  ```bash
  # Download llama-nexus.wasm
  curl -L https://github.com/LlamaEdge/LlamaEdge-Nexus/releases/latest/download/llama-nexus.wasm -o llama-nexus.wasm
  ```

  After decompressing the file, you will see the following files in the current directory.

  ```bash
  llama-nexus.wasm
  SHA256SUM
  ```

- Download LlamaEdge API Servers

  LlamaEdge provides four types of API servers:

  - `llama-api-server` provides chat and embedding APIs. [Release Page](https://github.com/LlamaEdge/LlamaEdge/releases)
  - `whisper-api-server` provides audio transcription and translation APIs. [Release Page](https://github.com/LlamaEdge/whisper-api-server/releases)
  - `sd-api-server` provides image generation and editing APIs. [Release Page](https://github.com/LlamaEdge/sd-api-server/releases)
  - `tts-api-server` provides text-to-speech APIs. [Release Page](https://github.com/LlamaEdge/tts-api-server/releases)

  To download the `llama-api-server`, for example, use the following command:

  ```bash
  curl -L https://github.com/LlamaEdge/LlamaEdge/releases/latest/download/llama-api-server.wasm -o llama-api-server.wasm
  ```

- Install WasmEdge Runtime

  ```bash
  # To run models on CPU
  curl -sSf https://raw.githubusercontent.com/WasmEdge/WasmEdge/master/utils/install_v2.sh | bash -s -- -v 0.14.1

  # To run models on NVIDIA GPU with CUDA 12
  curl -sSf https://raw.githubusercontent.com/WasmEdge/WasmEdge/master/utils/install_v2.sh | bash -s -- -v 0.14.1 --ggmlbn=12

  # To run models on NVIDIA GPU with CUDA 11
  curl -sSf https://raw.githubusercontent.com/WasmEdge/WasmEdge/master/utils/install_v2.sh | bash -s -- -v 0.14.1 --ggmlbn=11
  ```

- Start LlamaEdge-Nexus

  Run the following command to start LlamaEdge-Nexus:

  ```bash
  # Start LlamaEdge-Nexus with the default config file at default port 9069
  llama-nexus --config config.toml
  ```

  For the details about the CLI options, please refer to the [Command Line Usage](#command-line-usage) section.

- Register LlamaEdge API Servers to LlamaEdge-Nexus

  Run the following commands to start LlamaEdge API Servers first:

  ```bash
  # Download a gguf model file, for example, Llama-3.2-3B-Instruct-Q5_K_M.gguf
  curl -LO https://huggingface.co/second-state/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q5_K_M.gguf

  # Start LlamaEdge API Servers
  wasmedge --dir .:. --nn-preload default:GGML:AUTO:Llama-3.2-3B-Instruct-Q5_K_M.gguf \
    llama-api-server.wasm \
    --prompt-template llama-3-chat \
    --ctx-size 128000 \
    --model-name Llama-3.2-3b
    --port 10010
  ```

  Then, register the LlamaEdge API Servers to LlamaEdge-Nexus:

  ```bash
  curl --location 'http://localhost:9068/admin/servers/register' \
  --header 'Content-Type: application/json' \
  --data '{
      "url": "http://localhost:10010",
      "kind": "chat"
  }'
  ```

  If register successfully, you will see a similar response like:

  ```bash
  {
      "id": "chat-server-36537062-9bea-4234-bc59-3166c43cf3f1",
      "kind": "chat",
      "url": "http://localhost:10010"
  }
  ```

## Usage

If you finish registering a chat server into LlamaEdge-Nexus, you can send a chat-completion request to the port LlamaEdge-Nexus is listening on. For example, you can use the following command to send a chat-completion request to the port 9068:

```bash
curl --location 'http://localhost:9068/v1/chat/completions' \
--header 'Content-Type: application/json' \
--data '{
    "model": "Llama-3.2-3b",
    "messages": [
        {
            "role": "system",
            "content": "You are an AI assistant. Answer questions as concisely and accurately as possible."
        },
        {
            "role": "user",
            "content": "What is the capital of France?"
        },
        {
            "content": "Paris",
            "role": "assistant"
        },
        {
            "role": "user",
            "content": "How many planets are in the solar system?"
        }
    ],
    "stream": false
}'
``` -->

## Command Line Usage

LlamaEdge-Nexus provides various command line options to configure the service behavior. You can specify the config file path, enable RAG functionality, set up health checks, configure the Web UI, and manage logging. Here are the available command line options by running `llama-nexus --help`:

```bash
LlamaEdge Nexus - A gateway service for LLM backends

Usage: llama-nexus.wasm [OPTIONS]

Options:
      --config <CONFIG>  Path to the config file [default: config.toml]
      --rag              Use rag-api-server instances as downstream server instead of llama-api-server instances
      --web-ui <WEB_UI>  Root path for the Web UI files [default: chatbot-ui]
  -h, --help             Print help
  -V, --version          Print version
```

# LlamaEdge-Nexus

LlamaEdge-Nexus is a gateway service for managing and orchestrating LlamaEdge API servers. It provides a unified interface to various AI services including chat completions, audio processing, image generation, and text-to-speech capabilities. Compatible with OpenAI API, LlamaEdge-Nexus allows you to use familiar API formats while working with open-source models. With LlamaEdge-Nexus, you can easily register and manage multiple API servers, handle requests, and monitor the health of your AI services.

## Installation

- Download LlamaEdge-Nexus binary

  The LlamaEdge-Nexus binaries can be found at the [release page](https://github.com/LlamaEdge/LlamaEdge-Nexus/releases). To download the binary, you can use the following command:

  ```bash
  # Download
  curl -LO https://github.com/LlamaEdge/LlamaEdge-Nexus/releases/latest/download/llama-nexus-wasm32-wasip1.tar.gz

  # Extract the file
  tar -xzvf llama-nexus-wasm32-wasip1.tar.gz
  ```

  After decompressing the file, you will see the following files in the current directory.

  ```bash
  llama-nexus.wasm
  config.toml
  SHA256SUM
  ```

- Download LlamaEdge API Servers

  LlamaEdge provides four types of API servers:

  - `llama-api-server` provides chat and embedding APIs. [Release Page](https://github.com/LlamaEdge/LlamaEdge/releases)
  - `whisper-api-server` provides audio transcription and translation APIs. [Release Page](https://github.com/LlamaEdge/whisper-api-server/releases)
  - `sd-api-server` provides image generation and editing APIs. [Release Page](https://github.com/LlamaEdge/sd-api-server/releases)
  - `tts-api-server` provides text-to-speech APIs. [Release Page](https://github.com/LlamaEdge/tts-api-server/releases)

  In the following steps, we will use the `llama-api-server` as an example to show how to register a chat server into LlamaEdge-Nexus, and send a chat-completion request to the port LlamaEdge-Nexus is listening on.

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
  # Start LlamaEdge-Nexus with the default config file at default port 9068
  wasmedge --dir .:. llama-nexus.wasm --config config.toml
  ```

  For the details about the CLI options, please refer to the [Command Line Usage](#command-line-usage) section.

- Start and register a chat server

  Let's download `llama-api-server.wasm` first:

  ```bash
  curl -L https://github.com/LlamaEdge/LlamaEdge/releases/latest/download/llama-api-server.wasm -o llama-api-server.wasm
  ```

  Then, start the chat server:

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

  Now, register the chat server to LlamaEdge-Nexus:

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
            "content": "What is the location of Paris, France?"
        }
    ],
    "model": "Llama-3.2-3B",
    "stream": false
}'
```

The response will be:

```bash
{
    "id": "chatcmpl-ab10e548-1311-4f86-b84f-f5fb9e7a6773",
    "object": "chat.completion",
    "created": 1742954363,
    "model": "Llama-3.2-3B-Instruct",
    "choices": [
        {
            "index": 0,
            "message": {
                "content": "Paris, France is located in northern central France, roughly 450 km southeast of London.",
                "role": "assistant"
            },
            "finish_reason": "stop",
            "logprobs": null
        }
    ],
    "usage": {
        "prompt_tokens": 430,
        "completion_tokens": 20,
        "total_tokens": 450
    }
}
```

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

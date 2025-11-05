# Llama.cpp router

A lightweight Node.js server that proxies requests to Llama Server instances, manages model loading/unloading based on VRAM availability, and provides a simple OpenAI‑compatible HTTP API.

> [!NOTE]  
> This project is only compatible with AMD GPUs as of now.

---

## Table of Contents

- [Features](#features)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Configuration](#configuration)
- [Running the Server](#running-the-server)
- [API Endpoints](#api-endpoints)
- [Testing](#testing)
- [Contributing](#contributing)

---

## Features

- **Model routing** – Dynamically load/unload GGUF models on demand.
- **VRAM management** – Ensures a model fits into GPU memory, optionally evicting older unloadable models.
- **OpenAI compatible** – Supports `/v1/models`, `/v1/chat/completions`, and `/v1/embeddings` routes.
- **Hot‑reloading configuration** – Upload a new config JSON without restarting the server.
- **Streaming support** – Uses Hono's streaming API for Server‑Sent Events when `stream: true`.
- **Typed TypeScript codebase** – Full type safety with `zod` validation for configuration.

---

## Prerequisites

- **Node.js 24+** (the project is written as an ES‑module).
- **pnpm** (recommended) – you can also use npm or yarn, but the lockfile is for pnpm.
- The external binaries referenced in the configuration:
  - [llama-server](https://github.com/ggml-org/llama.cpp) – the Llama Server executable.
  - [gguf-parser-go](https://github.com/gpustack/gguf-parser-go) – tool used to estimate model memory usage.
  - `rocm-smi` – for ROCm GPU monitoring.

---

## Installation

```bash
# Clone the repository (if you haven't already)
git clone https://github.com/Quozul/llama_cpp_router.git
cd llama_cpp_router

# Install dependencies using pnpm
pnpm install
```

The `package.json` defines a few handy scripts:

- `pnpm run start` – launches the server using the `src/index.ts` entry point.
- `pnpm run test` – runs the test suite (`node --test`).
- `pnpm run typecheck` – runs `tsc --noEmit` to verify TypeScript types.
- `pnpm run check` – runs Biome linting (`pnpm exec biome check`).

---

## Configuration

The server expects a JSON configuration file (default path: `./config.json`). A **template** is provided as `config.example.json`. Copy it and adjust the values to match your environment:

```bash
cp config.example.json config.json
```

Key sections:

- `owner` – name displayed in the `/v1/models` response.
- `unloadDuration` – how many minutes a model may stay idle before being automatically unloaded.
- `system` – paths to external binaries.
- `server` – hostname and port the HTTP server will bind to.
- `models` – a record of model names and their individual configuration (model file path, network port, caching options, etc.).

You can reload the configuration at runtime by **POST**‑ing the new JSON to `/config`.

---

## Running the Server

```bash
# Using the default config file
pnpm run start
```

You can also specify an alternative config file path when invoking the entry point:

```bash
node src/index.ts ./my‑custom‑config.json
```

The server will start and print a line similar to:

```
🌐 Server listening on http://0.0.0.0:8080
```

---

## API Endpoints

All routes are prefixed with the path you configure the server to listen on (e.g. `http://localhost:8080`). The API mimics a subset of the OpenAI API.

| Method | Path                   | Description                                                              |
|--------|------------------------|--------------------------------------------------------------------------|
| `GET`  | `/v1/models`           | Returns a list of available models.                                      |
| `POST` | `/v1/chat/completions` | Proxy to Llama Server chat completions. Supports `stream: true` for SSE. |
| `POST` | `/v1/embeddings`       | Proxy to Llama Server embeddings endpoint.                               |
| `GET`  | `/modelFits`           | Returns VRAM fit information for all configured models.                  |
| `GET`  | `/config`              | Retrieves the current configuration JSON.                                |
| `POST` | `/config`              | Replaces the running configuration with the posted JSON.                 |

### Example: Get model list

```bash
curl http://localhost:8080/v1/models
```

Response (pretty‑printed):

```json
{
  "object": "list",
  "data": [
    { "object": "model", "id": "granite-4.0-nano", "owned_by": "llama.cpp" },
    { "object": "model", "id": "jina-embeddings-v4-text-retrieval", "owned_by": "llama.cpp" }
  ]
}
```

---

## Testing

The repository includes a small test suite that exercises the router and controller logic.
More tests should be added to ensure stability of this project.

```bash
pnpm run test
```

Tests are written using Node's built‑in `node:test` module and make use of `assert` and `mock` utilities.

---

## Contributing

Contributions are welcome! If you find a bug or want to add a feature:

1. Fork the repository.
2. Create a feature/bug‑fix branch.
3. Make your changes and run `pnpm run check` to ensure lint passes.
4. Run the test suite (`pnpm run test`).
5. Open a pull request.

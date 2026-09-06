# stimma-drawthings

A [Draw Things](https://drawthings.ai) backend for [Stimma](https://github.com/stimma-ai/stimma). Generate images and video locally, using the models you already have in Draw Things.

Stimma brings AI generation, an agent, and your media library together in a desktop app. We built this adapter to bring Draw Things into Stimma's generation tools and agent.

It uses the [Stimma Tools Protocol (STP)](https://github.com/stimma-ai/stimma-tools-protocol), our open protocol for connecting generation tools. That also makes it available to scripts, terminal workflows, and other applications through our [`stp` CLI](https://github.com/stimma-ai/stimma-tools-protocol-cli) or any STP client.

## Install

Download the archive for your platform from [Releases](https://github.com/stimma-ai/stimma-drawthings/releases/latest), unpack it, and put `stimma-drawthings` on your PATH.

Automatic engine setup requires **Apple Silicon and macOS 14 or later**. The adapter downloads the Draw Things engine and any missing models when you first generate. It shares the desktop app's model folder, so existing models are reused. The desktop app can stay closed.

On Linux, connect to an existing Draw Things server as described below.

## Run with Stimma

Start the adapter:

```sh
stimma-drawthings --websocket
```

In Stimma's tool provider settings, add a WebSocket provider with this URL:

```text
ws://127.0.0.1:8765/stp-v1
```

Leave the adapter running while you use it. It starts the generation engine when needed and keeps it loaded between requests. The first generation may take longer while models download.

If your Draw Things models live in a custom folder, start it with:

```sh
stimma-drawthings --websocket --models-dir /path/to/Models
```

## Use from the terminal

Our [STP CLI](https://github.com/stimma-ai/stimma-tools-protocol-cli#install) lets you use the same tools from scripts or your terminal, independently of the desktop app. Install it, then connect to the running adapter:

```sh
# See the available tools
stp --url ws://127.0.0.1:8765/stp-v1 tools

# See a tool's parameters
stp --url ws://127.0.0.1:8765/stp-v1 show z-image-turbo

# Generate an image and save it
stp --url ws://127.0.0.1:8765/stp-v1 run z-image-turbo \
  'a small orange ceramic robot on a teal tabletop' -o robot.png
```

`stimma-drawthings` runs the backend; `stp` sends it requests. Stimma and the CLI can both connect to the same running adapter.

## Tools and models

The adapter provides tools for image generation, editing, inpainting, outpainting, upscaling, and video generation. Supported models include FLUX, SDXL, Qwen Image, Z-Image, LTX-2.3, and Wan 2.2. Each tool has parameters suited to its model, including LoRA and ControlNet options where supported.

See the [tool catalog](docs/CATALOG.md) for the full list and coverage compared with ComfyUI, and [validation notes](docs/VALIDATION.md) for what has been tested. Newly added tools have schema checks but have not all been tested with live generation.

## Connect to an existing Draw Things server

To use the desktop app's engine, enable its gRPC server and Model Browsing, and disable Response Compression. Start the adapter with the address shown by Draw Things:

```sh
stimma-drawthings --websocket --endpoint http://127.0.0.1:7859
```

This also works with a separately running `gRPCServerCLI`, including one on another machine. Stimma and the STP CLI still connect to the adapter at the same WebSocket URL.

## Development

Install Rust and the [STP CLI](https://github.com/stimma-ai/stimma-tools-protocol-cli#install), then:

```sh
git clone https://github.com/stimma-ai/stimma-drawthings.git
cd stimma-drawthings
tools/drawthings build --locked
tools/drawthings test --locked
tools/drawthings lint
```

The binary is written to `target/release/stimma-drawthings`. Tests use a mock Draw Things server and the real STP CLI; they don't require model downloads or a GPU.

Run a development build with `tools/drawthings run --websocket`. Use `stimma-drawthings --help` for configuration options.

## License

[GPLv3](LICENSE.md). See [third-party notices](THIRD_PARTY_NOTICES.md) for the Draw Things engine and other components.

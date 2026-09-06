# stimma-drawthings

[Draw Things](https://drawthings.ai) as a standalone [Stimma Tools Protocol](https://github.com/stimma-ai/stimma-tools-protocol) provider. A small Rust executable for local image and video generation, usable by any STP host. **Stimma is optional.**

On macOS, the provider starts and stops its own Draw Things engine and uses the desktop app's model folder. No repeated API-server clicks, Python environment, inference framework, or duplicated model library. The engine, video encoder, LoRA converter, and missing official weights download when needed, with SHA-256 verification. Existing compatible weights are preferred.

## Install

Download and unpack the archive for your platform from [Releases](https://github.com/stimma-ai/stimma-drawthings/releases/latest). Put `stimma-drawthings` on your PATH. Checksums accompany each release.

The automatic local engine and LoRA-converter installation target **Apple Silicon macOS 14 or later**. Other platforms can build the provider and connect to an existing compatible Draw Things gRPC server. Inference speed and supported models depend on the engine and available memory.

To build from source:

```sh
git clone https://github.com/stimma-ai/stimma-drawthings.git
cd stimma-drawthings
tools/drawthings build --locked
# executable: target/release/stimma-drawthings
```

Rust/Cargo are build requirements, not runtime requirements. Protobuf generation uses a build-only vendored compiler; FlatBuffer bindings are checked in. The executable links only to system libraries on macOS.

## Quick start with the STP CLI

Install the [STP CLI](https://github.com/stimma-ai/stimma-tools-protocol-cli), then:

```sh
stp --exec 'stimma-drawthings' tools
stp --exec 'stimma-drawthings' show z-image-turbo
stp --exec 'stimma-drawthings' run z-image-turbo \
  'a small orange ceramic robot on a teal tabletop' \
  --width 512 --height 512 --seed 7 -o robot.png
```

The first generation installs the engine if needed, starts it on an unused loopback port, and checks the model's dependencies. Missing official weights are downloaded and transferred into Draw Things. First use can require several gigabytes; subsequent runs reuse the files. Progress notifications report installation and generation activity.

The stdio provider uses the host's `ASSET_PATH` directory. Exiting the host closes the provider and its owned engine. To keep the engine loaded across CLI invocations, run a persistent provider:

```sh
stimma-drawthings --websocket
stp --url ws://127.0.0.1:8765/stp-v1 tools
stp --url ws://127.0.0.1:8765/stp-v1 run z-image-turbo 'a blue ceramic robot' -o robot.png
```

WebSocket mode also serves input/output assets over HTTP. Use `--bind` to change the address. A non-loopback bind requires `STIMMA_DRAWTHINGS_TOKEN`; supply the same bearer token to your STP host. Use a TLS reverse proxy for access over an untrusted network.

## Desktop app or separate engine

**Managed mode (default):** the provider owns a separate upstream `gRPCServerCLI`. It shares the standard macOS desktop directory:

```text
~/Library/Containers/com.liuliu.draw-things/Data/Documents/Models
```

Set `--models-dir` if Draw Things uses a custom location. Model files are reused in place. The provider does not import or modify the desktop app's projects or canvas database. Its converted LoRA metadata is maintained in provider state; conversion does not automatically register a new LoRA in the desktop UI. Avoid deleting or replacing weights while either engine is using them.

**Attach mode:** enable the desktop app's gRPC server, enable Model Browsing, and disable Response Compression. Then connect:

```sh
stp --exec 'stimma-drawthings --endpoint http://127.0.0.1:7859' tools
```

Use the actual port shown by Draw Things. The same option connects to a standalone or remote `gRPCServerCLI`. `DRAWTHINGS_SHARED_SECRET` supplies its shared secret. The adapter does not start, stop, or change an attached server's settings. Managed mode uses loopback HTTP; attach mode also supports HTTPS with a trusted certificate. Self-signed desktop TLS is not configured automatically.

## Tools and native settings

The provider publishes **28 specific tools**. There are no generic image/video tools or arbitrary-checkpoint fallback. Each tool selects compatible checkpoints and exposes its own parameters. See the [ComfyUI coverage table](docs/CATALOG.md) for the 21 of 45 reference workflows covered at the model/task level and the remaining gaps.

- Z-Image Turbo, FLUX.1 Dev, FLUX.2 Dev/Klein 9B, Chroma HD, Anima Base, Krea 2 Turbo, Ideogram 4/Fast/Instant, SDXL, Qwen Image/2512 and Edit 2509/2511.
- Klein and SDXL inpainting/outpainting; SDXL Real-ESRGAN upscaling; SeedVR2 3B/7B image restoration.
- Separate LTX-2.3/Distilled text-to-video and image-to-video tools with audio; Wan 2.2 T2V/I2V with paired high/low-noise experts.
- Family-filtered LoRA selection and safetensors upload/conversion. Compatible ControlNets and preprocessed control images appear only where available.
- Progress, approximate latent previews, queued jobs, cancellation, and image/video asset transfer.

Inspect exact parameters with `stp show TOOL`, or emit all schemas with `stimma-drawthings --schemas`. `guidance` means CFG; FLUX Dev tools instead expose `guidance_embed` and fix CFG at 1. Edit/I2V/restoration tools require source images. Wan has no audio toggle. SeedVR2 fixes its one-step restoration settings and exposes target width/height rather than unused sampling controls. Source images are resized to the requested dimensions.

`native_configuration` contains a restricted per-tool set of advanced settings, such as tiling, caching, SDXL text conditioning, and LTX hires fix. It cannot override the selected model, sampler, frame count, or batching. Native tile/hires dimensions use **64-pixel units**. LTX uses 8n+1 frames (maximum 201); Wan uses 4n+1 (maximum 81). Duration and fps combinations exceeding those limits are rejected.

New profiles are schema/serialization-tested, **not individually live-generation validated**. The earlier live checks, including SDXL LoRA upload/conversion/generation, are documented in [validation](docs/VALIDATION.md).

Examples below use a persistent provider:

```sh
# Edit an image
stp --url ws://127.0.0.1:8765/stp-v1 run flux2-klein-9b \
  'make the robot blue, preserving its shape' --input_images robot.png -o blue.png

# White mask pixels regenerate; black pixels preserve the source
stp --url ws://127.0.0.1:8765/stp-v1 run flux2-klein-9b-inpaint \
  'a yellow ceramic robot' --input_images robot.png --mask mask.png -o edited.png

# Extend the canvas (source width/height are resized to the requested input dimensions)
stp --url ws://127.0.0.1:8765/stp-v1 run sdxl-outpaint \
  'a wider view of the room' --input_images robot.png \
  --width 512 --height 512 --outpaint_left 128 --outpaint_right 128 -o wider.png

# Native Real-ESRGAN; width/height are the input size before upscaling
stp --url ws://127.0.0.1:8765/stp-v1 run sdxl-upscale \
  --input_images robot.png --width 512 --height 512 -o larger.png

# Video with native audio; defaults use 1280x768, about 5 seconds, 24 fps
stp --url ws://127.0.0.1:8765/stp-v1 run ltx-2.3-distilled \
  'the robot waves hello, gentle ambient room sound' -o hello.mp4

# Existing LoRA
stp --url ws://127.0.0.1:8765/stp-v1 run sdxl 'a dramatic robot portrait' \
  --set 'loras:=[{"path":"sdxl_offset_v1.0_lora_f16.ckpt","weight":0.8}]' -o portrait.png

# Refresh discovery and inspect engine status
stp --url ws://127.0.0.1:8765/stp-v1 raw tools.refresh
stp --url ws://127.0.0.1:8765/stp-v1 raw drawthings.status
```

The STP CLI handles ordinary media uploads. Installing a safetensors LoRA uses `tools.upload`, HTTP/shared-directory transfer, and `tools.upload_complete`; this is supported by STP hosts with a LoRA upload UI. The provider refreshes the LoRA enum after installation.

This exposes the **native generation service**, not every desktop application feature or arbitrary ComfyUI workflows. Desktop canvas/project editing, training, model import/conversion beyond LoRAs, cloud billing/authentication, and UI-only operations are outside this provider. New or custom models may need installation in Draw Things first if no official checksum is available. A catalog entry is not a claim that every model variant and hardware combination has been tested. See [validation](docs/VALIDATION.md).

## Runtime management

`--state-path` changes the component/catalog/LoRA state directory. The default is `~/Library/Application Support/ai.stimma.drawthings-provider` on macOS; other systems use the user data directory. `--asset-path` overrides the STP asset directory. Persistent hosts should fetch/delete outputs they no longer need.

- `--engine PATH`: use an existing compatible engine executable in managed mode.
- `--lora-converter PATH` and `--ffmpeg PATH`: use installed runtime components.
- `--offline`: prohibit catalog, component, and model downloads; use previously installed files.
- `--endpoint URL`: attach to an independently managed engine.

Downloads use temporary files, verify SHA-256, and install atomically. Cancelled downloads discard staging files; a retry starts a fresh download. Models are staged only for transfer, then removed from provider staging. There is no separate persistent model cache. The next job restarts an owned engine if it exited. Cancelling a running generation drops its gRPC stream; the upstream engine stops at its next cancellation checkpoint. Closing an attached connection leaves that engine running.

## Development

```sh
tools/drawthings test --locked   # requires stp on PATH; uses mock gRPC, no inference
tools/drawthings lint
tools/drawthings generate        # requires flatc 25.12.19
```

CI tests stdio/WebSocket STP against a deterministic engine, including queue/cancel behavior and actual output pixels. Runtime tests cover schema/wire conversion, masks, safe asset access, and download verification. Live inference results and scope are recorded separately in [docs/VALIDATION.md](docs/VALIDATION.md).

## License

**GPLv3**, specifically GPL-3.0-only. See [LICENSE.md](LICENSE.md).

Draw Things is an independent project. Its engine, model weights, FFmpeg, and converter retain their own licenses. They are separate downloads, not embedded in the adapter executable. See [third-party notices](THIRD_PARTY_NOTICES.md), [dependency licenses](THIRD_PARTY_LICENSES.md), and [converter source/build instructions](vendor/lora-converter/README.md). Model licenses may impose conditions beyond the adapter's GPL license.

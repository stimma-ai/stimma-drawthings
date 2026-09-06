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

Profiles include Z-Image Turbo, FLUX.2 Klein 9B, Krea 2 Turbo, Ideogram 4 / Fast / Instant, SDXL, and LTX-2.3 / Distilled. Additional catalog models appear through `native-image` and `native-video` expert tools; those require model-appropriate settings rather than universal quality defaults.

- Text-to-image and compatible image-to-image models; multiple reference images for Klein.
- Masked inpainting and outpainting for Klein, SDXL, and compatible native image models.
- Text-to-video and image-to-video, with native generated audio encoded into MP4.
- LoRA selection, weights, and safetensors upload/conversion through STP's file-upload handshake.
- Paired control images and ControlNet/T2I/IP-Adapter metadata; supply the appropriate control image for the selected hint.
- Native upscalers, including `sdxl-upscale` for zero-strength upscaling without diffusion.
- Typed `native_configuration` for the pinned Draw Things generation schema: refiners, hires fix, tiling, separate text encoders, sampling controls, cache settings, and other native options.
- Execution progress, approximate latent previews for supported image formats, queued jobs, cancellation, and image/video asset transfer.

Inspect exact parameters and enums with `stp show TOOL`, or emit all schemas with `stimma-drawthings --schemas`. Unknown fields, invalid dimensions, invalid enum values, and mismatched LoRA/control families produce errors. Advanced settings still need to make sense for the selected engine model. Native tile and hires dimensions use **64-pixel units**. Videos use the engine's frame quantization and are limited to 201 frames.

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

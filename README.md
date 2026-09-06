# Draw Things STP provider — Rust prototype

A standalone native executable implementing STP over stdio and connecting to Draw
Things over gRPC. No Stimma application imports, Python installation, inference
library, model weights, converter, or FFmpeg binary are needed inside the adapter.

This is a working feasibility slice, **not the completed Draw Things provider**.
The release executable measured approximately 2.3 MiB on Apple Silicon. It links
only to macOS system libraries. Cargo dependencies are compiled into the binary;
the larger build directory and build-time protocol compiler are not shipped.

## Implemented

- STP registration, discovery/refresh, execution, progress, cancellation, queue status,
  and filesystem image assets.
- Engine-advertised Z-Image Turbo and SDXL text-to-image tools with explicit schema
  validation, checkpoint selection, dimensions, sampling steps, guidance, and seed.
- Protobuf/gRPC and generated FlatBuffer configuration; chunked uncompressed NNC
  tensor decoding and PNG output in Rust.
- Attach to an existing endpoint, or start an independently installed gRPCServerCLI
  as an owned child. The child binds to loopback with explicit model browsing and
  uncompressed responses. Engine output is kept off the STP stdout stream.
- Default macOS model path is the **Draw Things desktop model directory**. Models
  are read in place, without copying or importing the desktop database.
- Registration works when an attached engine is offline; `drawthings.status`
  reports readiness and the native model/LoRA/control/upscaler inventory. Inventory
  does not imply that all those operations are already executable through this slice.

## Build and test

```sh
tools/drawthings build
tools/drawthings test
tools/drawthings lint
```

Rust/Cargo are build requirements only. Tests require the `stp` CLI on PATH. The
integration test launches the actual provider executable and drives it through
`stp` against an in-process deterministic gRPC fixture. It validates discovery,
chunked tensor decoding, output pixels, and configuration fields. Normal tests
do not run inference or require an installed Draw Things application.

Protobuf bindings are generated at build time using a build-only vendored protoc.
The checked-in FlatBuffer bindings were generated using flatc 25.12.19:

```sh
tools/drawthings generate
```

## Use through STP

Attach to an already running Draw Things gRPC endpoint:

```sh
stp --exec 'target/release/stimma-drawthings --stdio' --cwd . tools
stp --exec 'target/release/stimma-drawthings --stdio' --cwd . show z-image-turbo
stp --exec 'target/release/stimma-drawthings --stdio' --cwd . raw drawthings.status
stp --exec 'target/release/stimma-drawthings --stdio' --cwd . \
  run z-image-turbo 'a small orange ceramic robot on a teal tabletop' \
  --width 512 --height 512 --seed 7 -o result.png
```

For managed execution, add `--engine /path/to/gRPCServerCLI-macOS` to the provider
command. An alternate local port uses `--endpoint http://127.0.0.1:17859`.
On macOS the desktop model directory is selected automatically; override it with
`--models-dir`. Other platforms require an explicit model directory and compatible
engine. The tested upstream engine version is `v1.20260716.0`.

In attach mode enable model browsing and disable response compression in Draw
Things. `DRAWTHINGS_SHARED_SECRET` supplies the existing server's shared secret
without putting it in the command line. TLS interoperability has not been live-tested.

## Live evidence

On 2026-09-05, the release binary launched the official macOS engine on a dedicated
loopback port against the existing desktop model directory. `stp tools` discovered
Z-Image Turbo and SDXL. A real Z-Image Turbo run using
`z_image_turbo_1.0_q8p.ckpt`, 512×512, 8 steps, seed 7 returned a PNG in approximately
17 seconds. Visual inspection confirmed the requested orange ceramic robot and teal
tabletop. No model download was needed. The owned engine no longer listened after
the STP host disconnected. Local artifacts live in ignored `.validation/`.

SDXL also passed a real STP generation using `sd_xl_base_1.0_q6p_q8p.ckpt`,
512×512, 16 steps, guidance 5, seed 7, in approximately 10 seconds. Both live runs
used the model-specific sampler defaults carried over from the Python reference.

## Remaining work

This branch has not yet ported image inputs/inpaint, video/audio, previews, LoRA
upload/conversion, controls, upscaling, training, WebSocket transport, or complete
model-family coverage. Engine/model downloads, checksum verification and atomic
installation, version management, and crash recovery are not implemented here.
The engine used for live validation was downloaded separately into a cache.

Cancellation is wired by aborting the task and dropping the gRPC stream; real engine
resource release and cancellation/next-job recovery still need live validation.
Startup awaits engine readiness before handling requests, so slow installation/startup
needs a separate asynchronous state machine before this is a finished manager.

Large runtime components should be fetched on demand, outside this binary. Finish
native Draw Things capability coverage and validate it through `stp` before integrating
the provider into Stimma onboarding or settings.

## License

GPL-3.0-only. See `LICENSE.md` and `THIRD_PARTY_NOTICES.md`.

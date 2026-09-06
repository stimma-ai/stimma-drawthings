# Validation

The standalone Rust provider is tested through the actual `stp` CLI against both a
deterministic gRPC fixture and the official Draw Things `v1.20260716.0` engine.
Live tests use Apple Silicon macOS with 128 GB unified memory and the desktop
Draw Things model directory. This validates the generation service, not every
model or desktop application feature.

## Automated checks

`tools/drawthings test --locked` covers:

- CLI discovery and generation over stdio and authenticated WebSocket.
- Chunked native tensor output, decoded image dimensions and exact pixels.
- Advanced configuration FlatBuffer roundtrips and invalid schema parameters.
- Image normalization and preserve/regenerate mask values.
- Queuing, cancellation of a queued request, exactly one terminal result, and recovery.
- Asset upload/download, unauthorized HTTP requests, and path/symlink confinement.
- Checksum rejection, atomic component installation, offline reuse, and corrupt-cache rejection.

`tools/drawthings lint` runs formatting and Clippy with warnings denied. CI runs
these checks on macOS and Linux; normal tests need no model weights or GPU.

## Live capability checks

Completed on 2026-09-05 using the Rust executable, STP asset transfers, and real
local inference. Unless specified otherwise, image tests use 512x512 and seed 7.
Elapsed times are not performance guarantees and exclude queue time.

| Capability | Tested result |
| --- | --- |
| Automatic engine install | Official binary downloaded, checksum verified, managed engine launched, Z-Image output returned |
| Z-Image Turbo | Text-to-image PNG; requested robot/table composition visually checked |
| Klein 9B | Image editing, masked inpainting, and expanded outpaint canvas |
| SDXL | Text-to-image and image-to-image |
| Krea 2 Turbo | Text-to-image with installed quantized checkpoint |
| Ideogram 4 | Base, Fast, and Instant text-to-image |
| Native LoRA | SDXL Offset selected with an explicit weight |
| LoRA upload | Official SDXL safetensors uploaded over STP/HTTP, converted, transferred, then used in generation; converter also auto-downloaded from the public runtime release |
| ControlNet | SDXL Canny Mid, paired control image, generated PNG |
| Native upscaling | Real-ESRGAN X2+, downloaded on first use; 512x512 input to 1024x1024 output |
| LTX-2.3 Distilled | Short video: 9 frames, 512x512, H.264, stereo 48 kHz AAC, 1.125 seconds |
| LTX default preset | 1280x768, 121 frames, 24 fps, 5.041667 seconds, stereo 48 kHz AAC; automatic encoder download |
| LTX-2.3 image-to-video | 512x512 source image, 9 output frames, native audio |
| Running cancellation | Cancel during sampling, one CANCELLED result, then successful Z-Image generation |
| Live previews | Five inline PNG previews received during the cancellation/recovery sequence |
| Packaged executable | Unpacked release archive generated through real STP in attach and offline managed modes |
| Process ownership | Owned engine exits with stdio provider; persistent WebSocket provider keeps its engine loaded |

The engine downloads model components into temporary staging and transfers them
through `UploadFile`; `FilesExist` confirms the result. The provider prefers
already installed checkpoint variants. Live outputs and raw traces remain local
and are not included in releases.

## Compatibility boundaries

The 28 named tools are checked for valid defaults, required media, rejected generic
IDs, family-filtered LoRAs, embedded versus CFG guidance, frame quantization,
expert pairing, and FlatBuffer configuration serialization. FLUX Dev, Chroma,
Anima, Qwen, Wan 2.2, and SeedVR2 additions have not been swept with live inference.
These checks establish parameter and transport behavior, not output quality or
successful inference on every new model. The live table above records the earlier
capability checks; the split LTX I2V IDs use that existing inference path.
Compression must be disabled on attached engines.
Trusted-certificate HTTPS transport is implemented; self-signed desktop TLS and
all remote hardware combinations have not been live-tested.

Training, desktop canvas/project operations, general checkpoint conversion,
cloud billing/authentication, and arbitrary ComfyUI workflows are outside this
provider's native-generation API boundary. Preview projection is approximate and
available only for recognized image latent layouts.

## Embedded manager

Verified real managed-engine start and stop through the browser, model search, connection details, and light/dark layouts at narrow widths. HTTP integration checks cover token-free network access, relative embedded assets, catalog counts, rejected unknown checkpoints, and operation completion. The manager shares the generation semaphore so setup and generation cannot run concurrently.

The revised manager was checked with a live Z-Image generation on Apple M4 Max: GPU utilization reached 99%, GPU allocations reached 8.3 GiB, and engine resident memory was reported. Native metrics are system-wide, sampled from `ioreg`, `vm_stat`, `sysctl`, and `ps` without elevated privileges. GPU fields are best-effort and may be unavailable on other macOS versions; remote engines do not inherit the adapter machine’s metrics. Cancelling an engine startup was verified to stop startup and release generation capacity.

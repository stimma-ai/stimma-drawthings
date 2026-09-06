Standalone Draw Things provider for any Stimma Tools Protocol host.

- Managed local engine with automatic component downloads and shared desktop model storage.
- Image generation/editing, inpainting, outpainting, ControlNet inputs, native LoRAs, and upscaling.
- LTX video generation with native audio and automatic MP4 encoding.
- Safetensors LoRA upload/conversion and typed advanced Draw Things configuration.
- Stdio and authenticated WebSocket transports, queued execution, progress, and cancellation.

GPLv3. No Stimma application, Python environment, or bundled model weights required.
Apple Silicon macOS supports automatic local engine/converter installation; the Linux
archive connects to an existing compatible gRPC engine. See the README and validation
document for tested configurations and native API boundaries.

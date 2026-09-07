# Third-party notices

## Draw Things

The Protobuf and FlatBuffer schemas, generated bindings, model profile metadata,
and native add-on catalogs derive from the GPLv3
[Draw Things community project](https://github.com/drawthingsai/draw-things-community).
The tested engine/schema version is `v1.20260716.0`.

- Protobuf: `Libraries/GRPC/Models/Sources/imageService/imageService.proto`.
- Configuration: the Draw Things generation FlatBuffer schema, carried over from
  the earlier interoperability adapter and generated with flatc 25.12.19.
- Add-ons: `Libraries/ModelZoo/Sources/{ControlNet,LoRA,Upscaler,TextualInversion}Zoo.swift`
  at the tested tag; model metadata/checksums also come from `models.drawthings.ai`.

The engine is downloaded directly from the official release. Its corresponding
source is available at [the same tag](https://github.com/drawthingsai/draw-things-community/tree/v1.20260716.0).
The optional LoRA converter has separate [exact source and patch provenance](vendor/lora-converter/README.md)
and a corresponding-source bundle on the `runtime-v1` release.

## Approximate previews

RGB projection matrices in `data/preview.json` derive from ComfyUI's
[`comfy/latent_formats.py`](https://github.com/Comfy-Org/ComfyUI/blob/e308cc73b466584b0c17be695e5de1a17438bb40/comfy/latent_formats.py),
under GPLv3. The Rust projection implementation does not include PyTorch or a
ComfyUI runtime. These matrices provide approximate latent previews, not final VAE decoding.

## FFmpeg

Video output is encoded with the FFmpeg already installed on the machine
(`--ffmpeg`, `STIMMA_DRAWTHINGS_FFMPEG`, or `ffmpeg` on PATH). No FFmpeg build
is downloaded or redistributed by this project.

## Rust dependencies

Exact versions are recorded in `Cargo.lock`. [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)
contains the compiled dependencies' license texts and attributions, generated
with `cargo-about` using `about.toml` and `tools/licenses.hbs`.
Build-only and test-only tools are not part of the shipped executable.

## Manager artwork

`manager-ui/public/drawthings.png` is a 128-pixel copy of the official Draw Things app icon, used to identify the connected provider. Draw Things branding belongs to Draw Things, Inc.

The manager uses Vue (MIT), with build dependencies Vite (MIT) and the Vue Vite plugin (MIT). See `manager-ui/package-lock.json` for the pinned build dependency graph. Its stylesheet mirrors the ComfyUI-Stimma manager, which shares this project's license.

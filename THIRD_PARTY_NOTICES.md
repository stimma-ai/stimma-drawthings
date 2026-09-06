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

The optional encoder download uses the public ImageIO FFmpeg builds pinned in
`data/encoders.json`. Each executable's SHA-256 is checked before installation.
The binaries are fetched directly from the
[ImageIO binary repository](https://github.com/imageio/imageio-binaries/tree/f8f64710ea88e7e4a352c0f7d8c0deac9f5fd685/ffmpeg).
See its [FFmpeg build project](https://github.com/imageio/imageio-ffmpeg),
[FFmpeg source](https://ffmpeg.org/download.html#get-sources), and the downloaded
binary's `-L` and `-buildconf` output for the GPL/LGPL terms and enabled codecs.
No FFmpeg executable is redistributed inside this repository or provider archive.
Users may instead supply `--ffmpeg`.

## Rust dependencies

Exact versions are recorded in `Cargo.lock`. [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)
contains the compiled dependencies' license texts and attributions, generated
with `cargo-about` using `about.toml` and `tools/licenses.hbs`.
Build-only and test-only tools are not part of the shipped executable.

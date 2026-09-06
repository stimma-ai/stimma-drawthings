# Tool coverage

The provider has 28 named tools. Compared with the 45-workflow ComfyUI catalog
reviewed on 2026-09-05, 21 have corresponding model/task coverage. This is not a
claim of matching sampling results, graph implementation, or every ComfyUI widget.

| ComfyUI workflows | Draw Things tools | Covered workflows |
| --- | --- | ---: |
| Z-Image Turbo | `z-image-turbo` | 1 |
| FLUX.1 Dev; FLUX.2 Dev | `flux1-dev`, `flux2-dev` | 2 |
| FLUX Klein generation/inpaint/outpaint | `flux2-klein-9b` and `-inpaint`/`-outpaint` | 3 |
| Chroma HD; Anima | `chroma-hd`, `anima` (Base 1.0) | 2 |
| Krea 2 Turbo; Ideogram 4 | `krea-2`, `ideogram-4` | 2 |
| SDXL | `sdxl` | 1 |
| Qwen Image/2512; Edit 2509/2511 | `qwen-image`, `qwen-image-2512`, `qwen-edit-2509`, `qwen-edit-2511` | 4 |
| LTX 2.3 T2V/I2V | `ltx-2.3`, `ltx-2.3-i2v` | 2 |
| Wan 2.2 T2V/I2V | `wan22-t2v`, `wan22-i2v` | 2 |
| SeedVR2 3B/7B image upscaling | `seedvr2-3b`, `seedvr2-7b` | 2 |

The other seven tools are Ideogram Fast/Instant, LTX 2.3 Distilled T2V/I2V,
SDXL inpaint/outpaint, and SDXL Real-ESRGAN upscaling.

## Remaining reference workflows

| Workflows | Count | Boundary |
| --- | ---: | --- |
| Krea RAW + Turbo | 1 | The ComfyUI two-pass graph is not implemented by the Turbo tool. |
| LTX 2.3 extend/loop/stitch | 3 | Temporal context, source-video handling and composition need additional execution paths. |
| LTX 2.5 | 5 | No corresponding native model found in the published engine or audited source. |
| MiniMax H3, including Turbo | 6 | No corresponding native architecture found in the audited engine/source. |
| RMBG 2.0 | 1 | Background-removal model is not exposed through this generation backend. |
| SeedVR2 video upscaling | 2 | Video input/batching and source-audio preservation are not implemented. |
| Sulphur 2 | 2 | No corresponding native model found. |
| Wan 2.2 FLF2V and three Lightning variants | 4 | First/last-frame conditioning and the matching Lightning expert recipes are not implemented. |

HiDream and Stable Cascade are intentionally excluded. Refreshing the upstream
catalog does not add generic tools or route unsupported architectures through an
unrelated profile. The known-failing Klein 9B i8x variant remains excluded.

## Parameter contract

Dimensions are pixels in multiples of 64. `guidance` is CFG; FLUX Dev uses
`guidance_embed` with CFG fixed at 1. `strength` is the image-to-image denoising
fraction where offered. Mask expansion/feathering and outpaint margins use pixels,
not ComfyUI's percentage/context controls. LoRAs use `path` and `weight`; choices
are filtered by the engine's model-version identifier. That filtering does not
prove every LoRA is compatible with every derivative in the same architecture.

Qwen Edit accepts up to three images; FLUX.2 Dev and Klein accept up to ten.
Additional references share one native shuffle hint with multiple tensors.
Wan automatically installs both q8p experts, exposes the sampling-schedule
`refiner_start` fraction, and limits output to 81 frames. SeedVR2 takes a single
image resized to the requested output dimensions, with fixed one-step sampling
and native Lab color calibration. It does not reproduce ComfyUI's long-edge sizing
or full video restoration pipeline.

## Upstream baseline and metadata

Checked directly on 2026-09-05: the latest published gRPC engine is
[v1.20260716.0](https://github.com/drawthingsai/draw-things-community/releases/tag/v1.20260716.0),
and the [desktop downloads page](https://drawthings.ai/downloads/) lists the same
version. The provider already pins that engine and verifies its checksum.

Also audited main commit
[d677eba](https://github.com/drawthingsai/draw-things-community/tree/d677eba2deae87ce916366713e27056b3ffaf3c6),
dated 2026-09-05. Its ModelVersion enum and built-in ModelZoo contain LTX 2.3 but no
LTX 2.5 or MiniMax H3 entries. This is a statement about the available native
implementation, not a prediction about future support or external API providers.

Embedded metadata combines the official models.drawthings.ai catalog with
selected built-in Qwen Edit 2511, Wan 2.2 and SeedVR2 specifications from the
[pinned ModelZoo](https://github.com/drawthingsai/draw-things-community/blob/v1.20260716.0/Libraries/ModelZoo/Sources/ModelZoo.swift).
The selected built-in checksums come from that same source. This allows full tool
discovery without launching an engine or refreshing the online catalog.

See [validation](VALIDATION.md) for the distinction between live-validated
capabilities and newly added schema/serialization-tested profiles.

28 specific Draw Things STP tools, replacing the four generic native tools.

Adds FLUX.1/2 Dev, Chroma HD, Anima Base, Qwen Image/2512 and Edit 2509/2511, Wan 2.2 T2V/I2V, SeedVR2 3B/7B image upscaling, and separate LTX 2.3/Distilled I2V tools. Built-in metadata allows complete discovery before engine startup.

Model-specific schemas distinguish CFG and embedded guidance, require editing inputs, filter LoRAs/ControlNets by architecture, restrict advanced settings, and use Wan expert pairing and frame limits. Multi-reference images now share one native shuffle hint.

Breaking changes: generic native-image/native-video tools are removed; LTX I2V uses dedicated -i2v IDs; native_configuration is restricted to per-tool options.

New tools are schema/serialization-tested, without a per-model live generation sweep. Existing live SDXL LoRA upload/conversion/generation validation remains documented. See docs/CATALOG.md for the 21/45 ComfyUI model/task coverage mapping and gaps.

The engine remains v1.20260716.0, verified as the latest published upstream release on 2026-09-05. LTX 2.5 and MiniMax H3 were not found in that engine or the audited latest source.

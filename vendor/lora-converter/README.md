# Draw Things LoRAConverter source provenance

The optional `runtime-v1/LoRAConverter-macos-arm64` release asset is GPL-3.0 software built from:

- `drawthingsai/draw-things-community` commit
  `eb4bc7f4e287e51d8d1a43ed86908127df536eac`
- its pinned `liuliu/s4nnc` commit `c49d9c1daa71fbceaf27f61e085a2c24df4b1a0d`
- s4nnc's pinned `liuliu/ccv` commit `9a91be647e57c89b88783a32853d66be4b1ad1a1`

The complete corresponding source is available from those public repositories. Apply
`sdk-compat.patch` to the Draw Things checkout and `ccv-sdk-compat.patch` to the pinned ccv checkout,
then build `//Apps:LoRAConverter` with the pinned Bazel version from Draw Things' `.bazelversion` and
`--config=release --macos_minimum_os=14.0` on arm64 macOS. The target currently reports a missing
dSYM after linking; the linked executable in `bazel-bin/Apps/LoRAConverter` is the packaged artifact.

The patches only spell two newer SDK enum values in forms accepted by the macOS 15.4 SDK. The
converter does not execute either device-capability/ANE inference path.

Packaged binary SHA-256:

```text
63e3086a8d72635b46a1f1b5f918b254a6bab7d48222c1d0f3418d5d20be74d4
```

Source downloads (also collected in the runtime release source bundle):

- [Draw Things source](https://github.com/drawthingsai/draw-things-community/archive/eb4bc7f4e287e51d8d1a43ed86908127df536eac.tar.gz)
- [s4nnc source](https://github.com/liuliu/s4nnc/archive/c49d9c1daa71fbceaf27f61e085a2c24df4b1a0d.tar.gz)
- [ccv source](https://github.com/liuliu/ccv/archive/9a91be647e57c89b88783a32853d66be4b1ad1a1.tar.gz)

Use the repository's pinned Bazel module dependencies and Xcode command-line tools.
Set a local Bazel override for ccv to the patched checkout. The source archives
include the build definitions, dependency versions, and upstream notices.
The adapter invokes this executable as a separate process; it is downloaded only
when a user uploads a safetensors LoRA. Existing Draw Things LoRAs need no converter.

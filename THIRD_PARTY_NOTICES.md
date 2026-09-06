# Third-party notices

`schemas/config.fbs` and `schemas/image_service.proto` are Draw Things interoperability
schemas carried over from the Python provider. They derive from the GPLv3 Draw Things
community repository at https://github.com/drawthingsai/draw-things-community.
`src/generated/config_generated.rs` and build-generated Protobuf/gRPC bindings derive
from those schemas. This provider is distributed under GPL-3.0-only.

The protobuf schema corresponds to upstream
`Libraries/GRPC/Models/Sources/imageService/imageService.proto`. The FlatBuffer schema
is the prototype's configuration snapshot, not a promise of compatibility with every
upstream version. The live-tested engine release is v1.20260716.0.

No Draw Things engine, converter, model weights, or FFmpeg binary is included in this
repository or adapter executable. Such components carry their own license and source
distribution requirements. Dependencies and exact resolved versions are listed in
Cargo.toml and Cargo.lock; a complete release attribution bundle remains packaging work.

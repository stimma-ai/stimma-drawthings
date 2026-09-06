# Development

- This is a standalone Rust STP provider. Keep it independent of Stimma application code.
- Use `tools/drawthings` for building, testing, linting, packaging, and releasing.
- Drive integration tests through the real `stp` CLI. Ordinary tests use a deterministic gRPC fixture; live inference is opt-in.
- Regenerate protocol bindings with `tools/drawthings generate`; do not hand-edit generated bindings.
- Keep runtime binaries, models, local outputs, and private machine details out of git.
- Any new downloadable executable must have pinned provenance, SHA-256 verification, and corresponding source/license notices.

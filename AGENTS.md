# Development

This branch explores a native Rust replacement for the Python provider.
Use `tools/drawthings` for build, test, lint, and execution.
Keep the provider independent of Stimma application code. Validate through `stp`.
Never hand-edit generated protocol bindings. Large engine/model/converter binaries
must not be checked in or linked into the adapter.

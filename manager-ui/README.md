# Draw Things manager UI

A Vue interface embedded in the Rust provider at `/stp-v1/manage/`. It mirrors the ComfyUI-Stimma manager: same layout, tokens, and controls, so both providers feel identical inside Stimma's provider popover. Relative asset/API URLs support Stimma's manager proxy; `stimma-theme`, `stimma-manage-refresh`, and `stimma-manage-size` messages integrate with its popover.

From the repository root, run `tools/drawthings ui` to rebuild the committed `dist/` assets, then `tools/drawthings run --websocket`. Node is only needed to edit the UI; users run the single provider executable.

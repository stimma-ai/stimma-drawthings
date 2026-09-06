# Draw Things manager UI

A Vue interface embedded in the Rust provider at `/stp-v1/manage/`. It follows Stimma's Atelier v3 palette, typography, spacing, and controls. Relative asset/API URLs support Stimma's manager proxy; `stimma-theme`, `stimma-manage-refresh`, and `stimma-manage-size` messages integrate with its popover.

From the repository root, run `tools/drawthings ui` to rebuild the committed `dist/` assets, then `tools/drawthings run --websocket`. Node is only needed to edit the UI; users run the single provider executable.

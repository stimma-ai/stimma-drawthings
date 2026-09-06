The Draw Things manager now matches the ComfyUI manager: same layout, controls, and Atelier palette inside Stimma's provider popover and in a browser.

Tools lists every supported model with its task types, shows which are downloaded, and downloads the rest with one click. Each tool has a dependency view (checkpoint, text encoders, autoencoder, experts) with sizes, missing files, and free disk space, plus the other published versions of the model. Overview shows GPU utilization, unified memory, memory pressure, engine memory, and running generations in the same tiles the ComfyUI manager uses.

Stimma only receives tools whose checkpoint is downloaded, and each tool offers only its downloaded checkpoint versions. Hosts that advertise STP `tool_status` receive the remaining tools flagged `needs_setup`. Nothing downloads on first generation; the manager owns downloads.

The provider icon is a small copy of the Draw Things app icon, embedded as a data URI so hosts show it without fetching anything.

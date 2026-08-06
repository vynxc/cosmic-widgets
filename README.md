# cosmic-widgets

Native-performance HTML/CSS desktop and edge widgets for the COSMIC desktop.

The renderer is designed around Blitz's DOM/layout/paint crates and direct WGPU/Vello presentation—not Chromium, WebKit, Electron, or a JavaScript runtime. Ordinary widgets use declarative `data-cw-*` bindings. Optional logic runs through Extism with WASI and direct host access disabled.

## What is implemented

- Versioned `.cwidget` manifest and archive format.
- ZIP traversal, symlink, expanded-size, active-content, and schema validation.
- Deny-by-default capability and persisted grant types.
- Declarative provider tree and DOM patch types.
- Lazy Extism adapter with memory, fuel, timeout, and output ceilings.
- Blitz DOM/style/layout-to-AnyRender scene pipeline and headless recording renderer.
- Live SCTK bottom-layer desktop surface with exact click-through input.
- Shared direct Wayland/WGPU device, premultiplied-alpha swapchain, and Vello presentation.
- COSMIC theme-to-CSS token bridge.
- Desktop/edge placement model and fractional-scale input-region geometry.
- Session D-Bus control service, CLI, applet controller, and service metadata.
- Clock, system monitor, media, and sticky-note packages.

`cosmic-widgets serve` currently mounts the bundled clock as the first live vertical slice. Loading every installed instance from the persisted layout, pointer-driven editing, and the full panel UI are the next host milestones.

## Build and test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p cosmic-widgets -- doctor
```

Run the service in a COSMIC Wayland session:

```sh
cargo run -p cosmic-widgets -- serve
cargo run -p cosmic-widgets-applet -- status
```

For a persistent user installation, copy the release binaries to `~/.local/bin`, install `data/cosmic-widgets.service` under `~/.config/systemd/user`, and enable it with `systemctl --user enable --now cosmic-widgets.service`.

See [architecture.md](docs/architecture.md) and [widget-format.md](docs/widget-format.md).

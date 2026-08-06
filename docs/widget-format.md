# `.cwidget` format v1

A package is a ZIP archive containing `widget.toml`, `index.html`, optional `style.css`, optional `logic.wasm`, and local assets.

JavaScript, script tags, iframes, object/embed content, absolute paths, archive traversal, and symlinks are rejected. Network, storage, media, metrics, clipboard, URI, and process operations require declared capabilities and user-owned grants.

## Declarative bindings

- `data-cw-text="clock.time"` replaces text content.
- `data-cw-class-active="media.playing"` toggles a class.
- `data-cw-style-value="system.cpu_percent"` updates an approved style value.
- `data-cw-on-click="media.toggle"` dispatches a host action.
- `data-cw-storage="note.text"` binds an input to instance-scoped storage.

Paths are property lookups, not expressions. Packages cannot evaluate source text.

## Extism logic

Advanced packages may include `logic.wasm`. The host calls versioned CBOR entrypoints such as `cw_init`, `cw_event`, `cw_tick`, `cw_suspend`, and `cw_resume`. WASI is disabled. All external effects go through capability-checked cosmic-widgets host functions.


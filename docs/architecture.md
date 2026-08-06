# Architecture

`cosmic-widgets` keeps unstable graphics dependencies behind small interfaces:

```text
COSMIC panel applet / CLI
            │ D-Bus
            ▼
       host runtime ───── Extism (lazy, no WASI)
         │       │
 providers     package permissions
         │
 declarative patches
         ▼
 renderer boundary ───── Blitz DOM / Stylo / Taffy / Parley
         │
 shared WGPU device ───── Vello / AnyRender
         │
 SCTK layer surfaces ──── one composite surface per output/policy
```

The stable interfaces are the package manifest, declarative attributes, capability grants, layout file, Extism CBOR messages, and D-Bus API. Blitz, Vello, Extism, and SCTK types must not cross those boundaries.

Desktop surfaces use the bottom layer with no exclusive zone. Edge groups are keyed by output, edge, and policy. Input regions contain interactive widget rectangles only, preserving normal desktop and panel interaction.

## Live renderer status

The first clock instance now runs through the SCTK event loop, Blitz DOM/layout, a reusable Vello scene, and the shared premultiplied-alpha WGPU swapchain. The remaining multi-instance completion gate is:

1. Share the existing device and caches across multiple output surfaces.
2. Complete fractional-scale protocol handling at 1.25 and 1.5 in addition to integer scales.
3. Enable interactive input regions and keyboard focus only while editing.
4. Load installed instances and positions from the persisted layout.
5. Verify stable memory after repeated output, theme, and document recreation.

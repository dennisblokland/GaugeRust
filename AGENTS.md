# GaugeRust — Agent Instructions

Real-time automotive gauge display for the RG35XX handheld (ARM64 Linux). Connects via BLE to a Motorsport Electronics ECU, decodes CAN bus frames, and renders live gauges on the CPU with tiny-skia, shown through SDL2. The repo is a Cargo workspace. The original C# app was removed; it is in the git history up to `7405d83`.

## Crates

| Crate | Purpose |
|---------|---------|
| `gauge-app` | `gauge` binary: device app, PNG snapshots, benchmark, Bluetooth tools |
| `gauge-render` | Library: canvas, fonts, custom element renderers, calculated channels, built-in gauges, `gauges.json` loading |
| `gauge-designer` | Desktop gauge designer (egui) that previews with the device renderer — see `docs/workspace.md` |
| `me-data` | CAN bus decoder (Motorsport Electronics frames `ME1_1`–`ME1_8`), `MeData`, data-source names |
| `me-ble` | Bluetooth link to the ECU, on btleplug |

## Build & Test

```shell
cargo test                                         # Default members: everything but the designer
cargo test --workspace                             # Also the designer
cargo run --release --features sdl -- app --demo   # Run the app with simulated data
```

Deploy to device: `REMOTE_HOST=<ip> ./deploy-arm64.sh` (builds linux-arm64, copies to the device; muOS or ROCKNIX is detected over ssh). The deploy script also uploads the Ports artwork from `muos/`: on muOS to `/run/muos/storage/info/catalogue/External - Ports/` (box art, preview, description), on ROCKNIX to `gamelist.xml` and `images/`.

ROCKNIX: see `rocknix/README.md`. The OS is detected at runtime (`JoystickLayout::detect`, `OS_NAME` in the launcher), not forked. It has a different joystick numbering (table in `gauge-app/src/input.rs`), no muOS Bluetooth bring-up, and its launcher code in `rocknix/launcher.sh`.

## Conventions

- **Indentation:** tabs, max width 120 (see `rustfmt.toml`)
- **Compatibility:** `gauges.json` and custom gauge JSON keep the format the C# app used (`$type` discriminators, PascalCase properties, .NET number and date format strings), so existing files keep loading. Don't change it without a migration path.
- **Channel names:** compare them with `gauge_render::eq_ignore_case` (Unicode, so `Öltemp` matches `ÖLTEMP`), not `eq_ignore_ascii_case`. Calculated channels can have non-ASCII names; only the ECU's own names in `me-data` are ASCII.
- **Image paths:** use `Element::image_path` / `image_path_mut` rather than matching on element types.
- **C# mentions in comments:** many Rust comments name the C# class a module came from, or explain a behaviour by comparing with it. The code they mention is in the git history.

## Architecture Notes

- `gauge-app/src/display.rs` — Creates the SDL2 window, streams frames into a texture, uploads only damaged rects
- `gauge-render/src/gauges/*.rs` — Built-in gauges cache static content as trimmed `Sprite`s and redraw what moves
- `gauge-render/src/gauges/eva/` — The five `Eva*` gauges; `mod.rs` holds their shared palette, screen frame and widgets (segment bars, segmented arcs, hexagons, chips, panels, seven-segment readouts). Kanji come from `text::jp_typeface`, a subset font: add new characters to the subset
- `gauge-app/src/feed.rs` — `Feed` supplies values from the ECU (`me-ble`) or `Simulator` (`sim.rs`), chosen by `DemoMode` / `--demo`, or by L1 + R1 while the link is down (`Feed::switch_to_demo` stops the link on its own thread, because dropping `BleLink` can block for seconds)
- SDL can only be initialised on one thread, so a second `#[test]` that opens a `Display` silently skips. Add SDL scenarios to `game_loop_navigates_resets_and_exits` in `gauge-app/src/app.rs`
- `me_data::decode` dispatches CAN frames by `Pid` to typed structs; `MeData` aggregates decoded values
- Fonts in `gauge-render/fonts/` are embedded in the binary

## Custom Gauge System

Drag-and-drop gauge designer → JSON definition → runtime rendering on device.

### Workflow

1. `cargo run --release -p gauge-designer` — opens the designer
2. Add/arrange elements, bind data sources, tweak properties
3. Save → `custom-gauge.json`
4. Reference in `gauges.json`: `{ "Gauge": { "Type": "Custom" }, "CustomDefinitionFile": "custom-gauge.json" }`

### Element Types

| Type | `$type` | Data-Driven | Description |
|------|---------|-------------|-------------|
| `ArcElement` | `arc` | Yes | Sweep arc with track, glow, dynamic fill |
| `ZoneArcElement` | `zonearc` | Yes | Arc with up to 3 colored bands + optional value pointer tick |
| `NeedleElement` | `needle` | Yes | Rotating needle with tail + hub (optional image needle) |
| `TextElement` | `text` | No | Static label (custom font) |
| `ValueDisplayElement` | `value` | Yes | Formatted numeric display |
| `TickRingElement` | `ticks` | No | Major/minor tick marks with labels |
| `CircleElement` | `circle` | No | Filled/stroked circle |
| `RectangleElement` | `rectangle` | No | Rectangle with optional corner radius |
| `LineElement` | `line` | No | Straight line between two points |
| `LinearBarElement` | `linearbar` | Yes | Horizontal/vertical fill bar |
| `WarningIndicatorElement` | `warning` | Yes | Threshold-triggered color change with label |
| `ImageElement` | `image` | No | Custom image (backgrounds, logos, overlays) with opacity + rotation |
| `GraphElement` | `graph` | Yes | Rolling time-series plot; buffer keyed by `Id` in `ElementRenderer::graph_buffers` |
| `LabelValueElement` | `labelvalue` | Yes | Compound label + large value display; optional background box; X/Y = center |
| `PeakMarkerElement` | `peak` | Yes | Tick at rolling peak angle; state keyed by `Id` in `ElementRenderer::peak_state`; `DecaySeconds=0` holds forever |
| `GifElement` | `gif` | Yes | Multi-frame image; value maps `MinValue`→`MaxValue` to frame `0`→`N-1`. Frames decoded in `images.rs` (honours per-frame blending); exported via raw file copy (not PNG re-encode) to preserve frames |
| `ClockElement` | `clock` | No | System time, local or UTC, formatted by `Format` (.NET format string, see `format.rs`). Ticks live in the running app (per-frame redraw), static in the designer preview |

`CustomGaugeDefinition` also supports:
- `BackgroundImage` — path drawn behind all elements
- `BackgroundImageMode` — `Stretch` (default) / `Fill` / `Fit` / `Center` / `Tile`
- `BackgroundImageOpacity` — byte 0–255
- `CalculatedChannels` — list of `CalculatedChannel` (`Name`, `Expression`); evaluated each frame by `expression.rs` before drawing; names appear in all DataSource dropdowns

All `GaugeElement` subclasses have:
- `Opacity` (byte, 0–255) — renderer wraps the element in `canvas.save_layer` when `< 255`
- `UseVisibility` / `VisibilitySource` / `VisibleAbove` / `VisibleBelow` — element is skipped when the named source value is outside `[VisibleAbove, VisibleBelow]`

`ArcElement` and `LinearBarElement` have `UseConditionalColor` + `WarnThreshold`/`WarnColor`/`DangerThreshold`/`DangerColor` for threshold-based color switching. `ArcElement` also has `AntiClockwise` (bool) to reverse sweep direction.

`ArcElement` and `LinearBarElement` have `SegmentCount` (0 = continuous) and `SegmentGap` (px; along the arc for arcs): the track and fill split into that many segments, and a segment lights once the value reaches its end, like an LCD or VFD bar. `examples/retro.json` uses them for an 80s digital dash.

### Key Files

- `gauge-render/src/model.rs` — Element models (serde, `$type` discriminator)
- `gauge-render/src/custom/renderer.rs` — `ElementRenderer`, core draw methods (shared by app + designer)
- `gauge-render/src/custom/incremental.rs` — Damage tracking, tiles, threads
- `gauge-render/src/custom/gauge.rs` — `CustomGauge`, smoothing (lerp)
- `me-data/src/mapper.rs` — Maps string names → `MeData` fields
- `gauge-designer/src/` — Designer: `app.rs` (UI), `geometry.rs` (bounds, hit test, resize, angle handles), `fields.rs` (one property-table line per element property), `package.rs` (save with `images/`)

### Smoothing

`CustomGauge` lerps display values toward target, frame-rate independent (the feel of 10 %/frame at 60 fps). Visual elements (arc, needle, bar) use smoothed values. Text/value/warning elements snap to target values.

### Needle Image Guide

When using `ImagePath` on a `NeedleElement`, the image is rotated around the element's (X, Y) pivot point. The image should be a vertical needle pointing **up** (12 o'clock), as the renderer applies rotation from that orientation.

| Property | Description | Recommendation |
|----------|-------------|----------------|
| `ImagePath` | Path to PNG/JPG/WebP (relative to JSON or absolute) | Use transparent PNG for best results |
| `ImageWidth` | Rendered width of the needle image (px) | Match the visual width of the needle artwork |
| `ImageLength` | Rendered length from pivot to tip (px) | Should match `Length` or the visible needle portion |
| `Length` | Still used for hit-test bounds when image is set | Keep equal to `ImageLength` |

**Image orientation:** The source image should be a vertical strip with the **tip at the top** and the **pivot point at the bottom edge**. The renderer draws it into a rect from `(-ImageWidth/2, -ImageLength)` to `(ImageWidth/2, 0)` relative to the pivot, then rotates.

**Recommended dimensions:** For a 640×480 gauge canvas, a needle image of roughly 20×180 px works well. Use a tall narrow PNG (e.g., 40×360 source scaled down to `ImageWidth: 20, ImageLength: 180`).

### Designer Keyboard Shortcuts

See the key table in `docs/workspace.md`.

## Tests

- `#[test]` unit tests next to the code, render tests in `gauge-render/tests/render.rs`
- CAN frame decoding tests use raw byte array payloads

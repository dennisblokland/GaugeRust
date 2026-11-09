# GaugeRust

Real-time automotive gauge display for the [RG35XX](https://anbernic.com) handheld (ARM64 Linux, muOS or [ROCKNIX](rocknix/README.md)). Connects via BLE to a Motorsport Electronics ECU, decodes CAN bus frames, and renders live gauges on the CPU with [tiny-skia](https://github.com/RazrFalcon/tiny-skia), shown through SDL2.

The app was originally written in C#. Rust replaced it; the C# code is in the git history up to commit `7405d83`.

![Rust](https://img.shields.io/badge/language-Rust-b7410e) ![Platform](https://img.shields.io/badge/platform-linux--arm64-green) ![License](https://img.shields.io/badge/license-MIT-blue)

## Features

- Live gauge rendering at 60 fps on the device, repainting only what changed
- BLE connection to Motorsport Electronics ECU (CAN bus frames ME1_1–ME1_8)
- Multiple built-in gauge styles: circular, sweep, needle, TurboXs, bar, digital, histogram, min/max, grid, and five EVA-style gauges (dial, honeycomb, readout, warning, gear)
- **Custom gauge designer** — drag-and-drop desktop app (`gauge-designer`) that exports JSON definitions for the device and previews with the renderer the device runs, pixel for pixel
- 17 custom element types: arcs, zone arcs, needles (with image support), text, value displays, label+value boxes, tick rings, bars, warning indicators, images, value-driven gifs, clocks, mini graphs, peak markers, and more
- **Calculated channels** — define virtual data sources as math expressions (`Rpm / 1000`, `OilTemp - Iat`) evaluated each frame
- **Conditional element visibility** — show/hide elements based on a data source value range
- **Grid snap** in designer (G key toggle, 10 px grid); **multi-select** with Shift+click for group drag/delete; **drag corner handles** to resize the selected element
- Arc elements support anti-clockwise fill direction
- Smooth value interpolation (lerp) for fluid needle/arc/bar animation
- Simulated ECU data for desktop development (`gauge app --demo`, or `DemoMode` in `gauges.json`), or switch to it with L1 + R1 while waiting for the ECU (a hint on screen shows the combo)

## Crates

The repo is a Cargo workspace. [docs/workspace.md](docs/workspace.md) is the full guide.

| Crate | Purpose |
|---------|---------|
| `gauge-app` | `gauge` binary: the device app, plus PNG snapshots, a benchmark and Bluetooth tools |
| `gauge-render` | Library: canvas, fonts, custom element renderers, calculated channels, built-in gauges, `gauges.json` loading |
| `gauge-designer` | Desktop gauge designer (egui) |
| `me-data` | CAN bus decoder (Motorsport Electronics frames ME1_1–ME1_8), `MeData`, data-source names |
| `me-ble` | Bluetooth link to the ECU, on btleplug |

## Prerequisites

- [Rust](https://rustup.rs) 1.80 or newer
- SDL2 development libraries for the window (`libsdl2-dev` on Debian/Ubuntu)
- `libdbus-1-dev` for Bluetooth on the desktop

## Build & Run

```shell
cargo test                                            # Run tests
cargo run --release --features sdl -- app --demo      # Run the app with simulated data
cargo run --release -p gauge-designer                 # Open the gauge designer
```

### Deploy to Device

```shell
REMOTE_HOST=<device ip> ./deploy-arm64.sh
```

Builds the linux-arm64 binary and copies it to the RG35XX. The target OS, muOS or [ROCKNIX](rocknix/README.md), is detected over ssh. It also uploads the artwork shown in the Ports list (box art, preview and description from [`muos/`](muos/README.md)). See [docs/workspace.md](docs/workspace.md#building-and-deploying-to-the-rg35xx) for toolchain options.

A `gauges.json` or custom gauge written for the C# app works as it is: copy it next to the binary (`/mnt/mmc/ports/GaugeRust` on muOS, `/storage/GaugeRust` on ROCKNIX). The deploy doesn't remove an existing C# install from the device.

## Architecture

```
┌─────────────────────────────────────────────┐
│               gauge-app (app.rs)            │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  │
│  │ display  │  │  input   │  │  screens  │  │
│  │ SDL2     │  │ js0 + KB │  │ FPS, etc. │  │
│  └────▲─────┘  └──────────┘  └───────────┘  │
│       │                                     │
│  ┌────┴──────────────────────────────────┐  │
│  │ gauge-render                          │  │
│  │ built-in gauges / CustomGauge         │  │
│  │ ElementRenderer (tiny-skia)           │  │
│  └────▲──────────────────────────────────┘  │
│       │                                     │
│  ┌────┴─────┐  ┌──────────────────────────┐ │
│  │  feed    │  │ me-data                  │ │
│  │ BLE/Sim  │◄─┤ ME1_1..ME1_8 → MeData    │ │
│  └──────────┘  └──────────────────────────┘ │
└─────────────────────────────────────────────┘
```

- **display** creates the SDL2 window and streams frames into a texture, uploading only the damaged rects
- **Built-in gauges** cache static content as trimmed sprites and redraw only what moves; **CustomGauge** tracks damage per element
- **feed** supplies values from the ECU (`me-ble`) or the simulator (`sim`), chosen by `DemoMode` / `--demo`
- **me-data** decodes CAN frames by PID into typed structs; `MeData` aggregates all decoded values

## TurboXs Gauge

`"Type": "TurboXs"` is a handheld-style analog + digital display that works for any data source: a 270° dial on the left (`MinValue` at 6 o'clock, sweeping clockwise to `MaxValue` at 3 o'clock) and a large italic readout on the right.

- **Scale:** 4 to 8 major ticks, chosen so every label is a round number (−10…20 → steps of 5, 0…8000 → 2000, 40…140 → 20).
- **Title:** one word per line, so `"Boost Press."` becomes two lines. `Unit` goes under the readout.
- **Fitting:** long titles, values and labels shrink to fit, so 4-digit RPM readings stay clear of the dial. The readout (and Peak/Min) is sized for the widest value in `MinValue`–`MaxValue`, with fixed-width digits, so it keeps one size and position as the value changes.
- **Status block** (under the title):

  | Setting | Values | Default |
  |---|---|---|
  | `StatusMode` | `Peak` (highest value seen), `Min` (lowest), `Text` (fixed text with a blue dot), `None` | `Peak` |
  | `StatusLabel` | First line | `Peak:`, `Min:` or `Datalog:` (by mode) |
  | `StatusText` | Second line in `Text` mode | `Stop` |

  Peak and Min start at the first reading. R1 resets them, as on the other gauges. `StatusMode` can also be changed in the on-device config editor.
- **Old LCD look:** `"Lcd": true` imitates a backlit handheld LCD. The gauge renders at a third of the resolution with a faint pixel grid, black becomes a deep navy and white a slightly blue white, and moving parts leave a short trail. `LcdGhosting` sets how much of the previous frame lingers: 0 turns the trail off, and the maximum is 0.9 (default 0.2). Both can be changed in the on-device config editor. It costs about 0.4 ms per frame over the normal look (see `bench/RESULTS.md`).
- **Colours and fonts:** `ColorHex` is ignored, because the dial has a fixed white, grey and red palette. Text uses the bundled Aldrich and Saira Extra Condensed fonts (SIL OFL 1.1, licenses in `gauge-render/fonts`), which are also available to custom gauges.

```json
{ "Gauge": { "Type": "TurboXs", "DataSource": "Map", "Title": "Boost Press.", "Unit": "Psi", "MinValue": -10, "MaxValue": 20, "Decimals": 1 } }
{ "Gauge": { "Type": "TurboXs", "DataSource": "Rpm", "Title": "Engine Speed", "Unit": "RPM", "MaxValue": 8000 } }
{ "Gauge": { "Type": "TurboXs", "DataSource": "Map", "Title": "Boost Press.", "Unit": "Psi", "MinValue": -10, "MaxValue": 20, "Decimals": 1,
             "StatusMode": "Text", "StatusLabel": "Datalog:", "StatusText": "Stop", "Lcd": true, "LcdGhosting": 0.2 } }
```

![TurboXs gauge](docs/turboxs-gauge.png) ![TurboXs gauge with Lcd](docs/turboxs-gauge-lcd.png)

## EVA Gauges

Five full-screen gauges in one style: black background with a faint honeycomb, orange line work, seven-segment readouts, a kanji label for each ECU channel, and a `SYSTEM STATUS` line that turns to a blinking red WARNING past the limit. They share one set of drawing widgets (`gauge-render/src/gauges/eva/`), and each binds to one `DataSource` like the other built-in gauges.

| Type | Shows |
|------|-------|
| `EvaDial` | 270° segmented ring with ticks, a red zone from `WarnValue` and a centre readout. `LIMIT` blinks in the zone. Ranges of 2000 and up label the scale in thousands. |
| `EvaHoneycomb` | Four rows of hexagon cells that light up to the value: green below 0 (vacuum on a relative boost channel), orange, red above `WarnValue` |
| `EvaReadout` | Large readout, three zone chips and a scale bar. On an AFR channel the zones are RICH / STOICH / LEAN (14.2 and 15.2 by default) and lambda is shown; on others LOW / NORMAL / HIGH. |
| `EvaWarning` | Vertical segment bar and a badge: 警告 WARNING (red, blinking) at or above `WarnValue`, else 正常 NOMINAL |
| `EvaGear` | Large gear digit and a column of positions. 0 is N, below 0 is R; `MaxValue` from 1 to 10 sets the forward gears (else 6). |

| Setting | Meaning |
|---|---|
| `MinValue`, `MaxValue` | Scale range |
| `WarnValue` | The limit: redline, overboost, overheat, or the lean edge on `EvaReadout`. Unset takes a share of the range. |
| `WarnLowValue` | `EvaReadout` only: the lower zone edge (rich below it) |
| `Title` | Overrides the English title; the kanji comes from the data source |
| `Unit`, `Decimals` | As on the other gauges |
| `LogoImage` | Optional image (PNG, JPG, WebP, ...) drawn in a free corner of each screen; relative paths are next to the executable. SVG isn't supported, so export a PNG with a transparent background. |

`ColorHex` is ignored: the palette is fixed. The kanji are drawn with Noto Serif JP Black (SIL OFL 1.1, license in `gauge-render/fonts`), subset to the labels in `gauges/eva/mod.rs`; a new label needs its characters added to the subset. See `examples/eva.json`.

```json
{ "Gauge": { "Type": "EvaDial", "DataSource": "Rpm", "Unit": "RPM", "MaxValue": 9000, "WarnValue": 7000 } }
{ "Gauge": { "Type": "EvaWarning", "DataSource": "Clt", "Unit": "°C", "MinValue": 40, "MaxValue": 120, "WarnValue": 105 } }
```

## Custom Gauge System

Design gauges on your desktop, deploy JSON to the device.

### Workflow

1. Run the designer: `cargo run --release -p gauge-designer`
2. Add and arrange elements, bind data sources, tweak properties
3. Save → `custom-gauge.json`
4. Reference in your `gauges.json`:
   ```json
   { "Gauge": { "Type": "Custom" }, "CustomDefinitionFile": "custom-gauge.json" }
   ```

### Element Types

| Type | JSON `$type` | Data-Driven | Description |
|------|-------------|-------------|-------------|
| Arc | `arc` | Yes | Sweep arc with track, glow, dynamic fill |
| Zone Arc | `zonearc` | Yes | Arc with up to 3 colored bands (green/yellow/red zones) + optional value pointer |
| Needle | `needle` | Yes | Rotating needle with tail + hub (optional image) |
| Text | `text` | No | Static label with custom font |
| Value Display | `value` | Yes | Formatted numeric display |
| Tick Ring | `ticks` | No | Major/minor tick marks with labels |
| Circle | `circle` | No | Filled/stroked circle |
| Rectangle | `rectangle` | No | Rectangle with optional corner radius |
| Line | `line` | No | Straight line between two points |
| Linear Bar | `linearbar` | Yes | Horizontal/vertical fill bar |
| Warning Indicator | `warning` | Yes | Threshold-triggered color change with label |
| Image | `image` | No | Background, logo, or overlay with opacity + rotation |
| Graph | `graph` | Yes | Rolling time-series plot with configurable depth, line + fill |
| Label Value | `labelvalue` | Yes | Compound label + large value display with optional background box |
| Peak Marker | `peak` | Yes | Tick at rolling peak value on arc ring; optional decay timeout |
| Gif | `gif` | Yes | Multi-frame image; frame selected by value (Min→Max maps to first→last frame) |
| Clock | `clock` | No | System time, custom `Format` string; local or UTC; optional box |

### Background Image

The canvas supports a background image behind all elements. Set `BackgroundImage` to a path (relative to the JSON file or absolute). Control how it fills the canvas with `BackgroundImageMode`:

| Mode | Behaviour |
|------|-----------|
| `Stretch` | Stretch to fill canvas, ignoring aspect ratio (default) |
| `Fill` | Scale proportionally to cover canvas, crops edges |
| `Fit` | Scale proportionally to fit inside canvas, letterboxes |
| `Center` | No scaling, centered |
| `Tile` | Tile at original size |

`BackgroundImageOpacity` (0–255) controls transparency.

### Needle Image Support

Needle elements can use a custom image instead of a drawn line. The image should be a vertical strip pointing **up** (12 o'clock) with the pivot at the bottom edge. The renderer rotates it around the element's (X, Y) point.

For a 640×480 canvas, a needle image of roughly 20×180 px works well. Use a transparent PNG for best results.

### Value Smoothing

Display values are interpolated toward the target at 10% per frame for fluid animation. Visual elements (arc, needle, bar) use the smoothed value; text/value/warning elements snap to the actual value.

## Tests

```shell
cargo test
```

Unit tests verify CAN frame decoding against raw byte array payloads. Render tests in `gauge-render/tests` check drawn output, including that incremental and threaded frames match a full render.

## License

See [LICENSE](LICENSE) for details.

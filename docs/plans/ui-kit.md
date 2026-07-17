# Shared UI Kit — Mutualize Asset-Pack UI Between SDL and wgpu

Audit (2026-07-17): both renderers load the same UI assets, but wgpu skips
them where assembly is fiddly. SDL is the visual reference.

| Surface | SDL | wgpu |
|---|---|---|
| HP/authority bars, minimap frame, screen overlays | asset pack | parity |
| Zone name labels (in-world) | SmallRibbons plate + 3-slice bar | plain text + flat rect |
| Unit order labels | SmallRibbons plate | plain text |
| Victory progress banner | 3-slice bar display | missing entirely |
| Zone pips panel | SpecialPaper full-scale | SpecialPaper squeezed 0.2× |
| Touch buttons | flat circles | flat circles |

Root cause: the assembly math (9-slice, 3-slice, ribbon cells) is
re-implemented in each renderer's `draw_helpers.rs`; core `render_util`
only holds the constants. Every widget costs two implementations, so the
second one gets the cheap version.

## Design

### Core assemblers (`render_util` additions)

Pure geometry: widgets compute `Vec<UiQuad>` + text ops; renderers blit.

```rust
pub enum UiAsset { SpecialPaper, BarBase, BarFill, BigRibbons, SmallRibbons,
                   RoundButtonBlue, RoundButtonBluePressed,
                   RoundButtonRed, RoundButtonRedPressed, ButtonIcons }
pub struct UiQuad { pub asset: UiAsset, pub src: SrcDst,
                    pub tint: Option<(u8, u8, u8)> }
```

- `panel_quads(nine, atlas_w, atlas_h, x, y, w, h, scale)` — unifies SDL
  `draw_panel` and wgpu `draw_panel_scaled` (scale=1 for the former).
- `bar_quads(bw, bh, x, y, w, h, cap_w, fill_ratio, tint)` — 3-slice base
  + tinted fill strip.
- `ribbon_quads(kind, color_row, x, y, w, h)` — big + small ribbons.
- `round_button_quads(color, pressed, icon_index, cx, cy, radius)` — round
  base (pressed art swap) + centered icon at ~55% size, nudged down when
  pressed (the pressed art is squashed).

### Shared widgets (drift-proofing the surfaces that diverged)

`zone_label_widget`, `order_marker_widget`, `victory_banner_widget`,
each returning quads + `UiText { text, x, y, size, color }` ops — SDL's
current look is the spec. Both renderers call the same widget.

### Renderer side

Each renderer keeps: `UiAsset → texture handle` mapping + a ~10-line quad
blit loop. Existing `draw_helpers` functions become thin wrappers over
core assemblers (call sites unchanged), then the three missing wgpu
surfaces are added via the shared widgets.

### Stylized touch buttons (asset survey done)

Pack provides SmallBlue/RedRoundButton Regular+Pressed (128×128) and
Icons 01–12 (64×64). Mapping: Attack = red + Icon_05 (sword); Charge =
blue + Icon_08 (arrow); Defend = blue + Icon_06 (shield); Dismiss = blue
+ Icon_09 (X). Dismiss keeps the hold-fill ring drawn over the art.
Reserved for later: Icon_10 (gear) settings, Icon_12 (note) audio,
Avatars on the death screen.

## Roadmap

- [x] 1. Core assemblers: `compute_scaled` (unifying the panel variants),
  `bar_base_quads`, `bar_fill_quad`, `big_ribbon_quads`,
  `small_ribbon_quads`, `round_button_quads` + 6 unit tests. (The
  `UiAsset`/`UiQuad` indirection proved unnecessary — renderers pass
  their own texture handle next to the shared quads.)
- [x] 2. Both `draw_helpers.rs` rewritten as thin blitters over core
  (`blit`/`blit_tinted`); wgpu file went from 407 inline lines to
  delegates; call sites unchanged.
- [x] 3. wgpu gap closure: SmallRibbons zone name plates + 3-slice
  signed capture bars, SmallRibbons order-acknowledgement plates
  (replacing the bare "!"), victory progress banner (was missing).
- [x] 4. Zone pips panel unified to SDL's full-scale parchment
  (pip_r 14 / gap 12 / pad 18 layout, plain `draw_panel`).
- [x] 5. Touch buttons: SmallBlue/RedRoundButton Regular+Pressed with
  icon overlays (sword/shield/arrow/X) in both renderers, circle
  fallback when assets missing; dismiss hold ring kept on top.
- [x] 6. Verified: 198 tests, clippy + fmt clean, SDL and wgpu-wasm
  builds green. Layout arithmetic for the three ported surfaces is
  still per-renderer (world-space vs screen-space coordinates differ);
  the assembly math is single-source in core.

Out of scope for now: WoodTable/Swords menu polish, avatars, cursors,
Banner.png headers (own pass later).

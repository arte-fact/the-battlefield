# Asset Pack Reference -- Tiny Swords (Free Pack)

**Source:** [Tiny Swords by Pixel Frog](https://pixelfrog-assets.itch.io/tiny-swords) (itch.io)
**Style:** Top-down pixel art, chibi proportions
**Base tile size:** 64x64 pixels
**Unit frame size:** 192x192 pixels (Lancer: 320x320)
**Format:** PNG sprite sheets (horizontal strips), some with Aseprite source files

## Directory Structure

```
assets/Tiny Swords (Free Pack)/
├── Units/                  # Animated unit sprites (5 factions x 5 unit types)
├── Buildings/              # Static building sprites (5 factions x 8 building types)
├── Terrain/
│   ├── Tileset/            # Ground and water tilemaps
│   ├── Decorations/        # Bushes, rocks, clouds, water rocks
│   └── Resources/          # Gold, wood, meat, tools
├── UI Elements/            # Buttons, bars, icons, avatars, cursors
└── Particle FX/            # Dust, explosions, fire, water splash
```

## Factions (Color Variants)

All units and buildings come in **5 color variants**:

| Faction | Primary use |
|---------|-------------|
| **Blue** | Player army (default) |
| **Red** | Enemy army (default) |
| **Purple** | Alternative army |
| **Yellow** | Alternative army |
| **Black** | Alternative army |

For the battle, two factions are selected per run. Blue vs Red is the default matchup.

## Units

Each faction has **5 unit types**. All sprites are horizontal strip sprite sheets.

### Warrior (Sword & Shield Infantry)

**Frame size:** 192x192 | **Role:** Frontline melee fighter

| Animation | File | Dimensions | Frames |
|-----------|------|:---:|:---:|
| Idle | `Warrior_Idle.png` | 1536x192 | 8 |
| Run | `Warrior_Run.png` | 1152x192 | 6 |
| Attack 1 | `Warrior_Attack1.png` | 768x192 | 4 |
| Attack 2 | `Warrior_Attack2.png` | 768x192 | 4 |
| Guard | `Warrior_Guard.png` | 1152x192 | 6 |

**Notes:** Has two distinct attack animations. Guard animation can be used for blocking/defending. Carries a sword and shield -- visually reads as balanced infantry.

### Archer (Ranged Unit)

**Frame size:** 192x192 | **Role:** Ranged attacker

| Animation | File | Dimensions | Frames |
|-----------|------|:---:|:---:|
| Idle | `Archer_Idle.png` | 1152x192 | 6 |
| Run | `Archer_Run.png` | 768x192 | 4 |
| Shoot | `Archer_Shoot.png` | 1536x192 | 8 |

**Extra asset:** `Arrow.png` (64x64) -- projectile sprite for arrow in flight.

**Notes:** Shoot animation has 8 frames showing the full draw-aim-release cycle. Arrow projectile needs rotation based on direction.

### Lancer (Mounted Cavalry)

**Frame size:** 320x320 | **Role:** Mounted melee, charge attacks

| Animation | File | Dimensions | Frames |
|-----------|------|:---:|:---:|
| Idle | `Lancer_Idle.png` | 3840x320 | 12 |
| Run | `Lancer_Run.png` | 1920x320 | 6 |

**Directional attack and defence** (5 directions each):

| Direction | Attack | Frames | Defence | Frames |
|-----------|--------|:---:|---------|:---:|
| Up | `Lancer_Up_Attack.png` | 3 | `Lancer_Up_Defence.png` | 6 |
| Up-Right | `Lancer_UpRight_Attack.png` | 3 | `Lancer_UpRight_Defence.png` | 6 |
| Right | `Lancer_Right_Attack.png` | 3 | `Lancer_Right_Defence.png` | 6 |
| Down-Right | `Lancer_DownRight_Attack.png` | 3 | `Lancer_DownRight_Defence.png` | 6 |
| Down | `Lancer_Down_Attack.png` | 3 | `Lancer_Down_Defence.png` | 6 |

**Notes:** Only unit with directional attack sprites. Larger frame size (320x320) to accommodate the mount. Mirror horizontally for left-facing directions. Visually reads as a knight on horseback with a lance.

### Pawn (Worker / Militia)

**Frame size:** 192x192 | **Role:** Conscript, resource carrier

| Animation | File | Dimensions | Frames |
|-----------|------|:---:|:---:|
| Idle | `Pawn_Idle.png` | 1536x192 | 8 |
| Run | `Pawn_Run.png` | 1152x192 | 6 |

**Tool/resource variants** (same frame counts as base):

| Variant | Idle | Run | Interact |
|---------|:---:|:---:|:---:|
| Axe | 8 | 6 | 6 |
| Hammer | 8 | 6 | 3 |
| Knife | 8 | 6 | 4 |
| Pickaxe | 8 | 6 | 6 |
| Gold (carrying) | 8 | 6 | -- |
| Meat (carrying) | 8 | 6 | -- |
| Wood (carrying) | 8 | 6 | -- |

**Notes:** Most versatile unit visually. The base (unarmed) pawn can serve as a militia/conscript. Tool variants provide visual diversity in armies. Interact animations show the pawn using the tool (chopping, hammering, etc.).

### Monk (Healer / Support)

**Frame size:** 192x192 | **Role:** Healer, support caster

| Animation | File | Dimensions | Frames |
|-----------|------|:---:|:---:|
| Idle | `Idle.png` | 1152x192 | 6 |
| Run | `Run.png` | 768x192 | 4 |
| Heal | `Heal.png` | 2112x192 | 11 |

**Extra asset:** `Heal_Effect.png` (2112x192, 11 frames) -- particle effect overlay for the heal ability.

**Notes:** Heal animation is the longest at 11 frames. The separate heal effect sprite can be rendered on the target unit.

## Buildings

Each faction has **8 building types**. All are static (non-animated) PNG sprites.

| Building | Dimensions | Grid footprint (est.) | Description |
|----------|:---:|:---:|-------------|
| Castle | 320x256 | 5x4 tiles | Main fortification, large with gate |
| Barracks | 192x256 | 3x4 tiles | Military training building |
| Archery | 192x256 | 3x4 tiles | Archery range |
| Tower | 128x256 | 2x4 tiles | Defensive watchtower |
| Monastery | 192x320 | 3x5 tiles | Tall religious building |
| House 1 | 128x192 | 2x3 tiles | Small dwelling |
| House 2 | 128x192 | 2x3 tiles | Small dwelling (variant) |
| House 3 | 128x192 | 2x3 tiles | Small dwelling (variant) |

**Notes:** Building footprints are approximate. The isometric-ish top-down perspective means buildings extend upward visually beyond their ground footprint. Castle and Tower share a consistent architectural style per faction.

## Terrain

### Tilemaps

**Tile size:** 64x64 | **Tilemap size:** 576x384 (9 columns x 6 rows)

| File | Description |
|------|-------------|
| `Tilemap_color1.png` | Green grass + stone elevation |
| `Tilemap_color2.png` | Color variant 2 |
| `Tilemap_color3.png` | Color variant 3 |
| `Tilemap_color4.png` | Color variant 4 |
| `Tilemap_color5.png` | Color variant 5 |
| `Shadow.png` | 192x192 shadow overlay for elevated terrain |
| `Water Background color.png` | 64x64 base water tile |
| `Water Foam.png` | 3072x192 animated water edge (16 frames at 192x192) |

Each tilemap contains tiles for:
- **Ground** -- flat terrain pieces (corners, edges, fills)
- **Elevation** -- raised terrain / cliffs with side faces
- Tiles are designed for auto-tiling (edges, corners, inner corners)

### Decorations

| Category | Files | Size per sprite | Description |
|----------|:---:|:---:|-------------|
| Bushes | 4 variants | ~128x128 (in 1024x128 strip, ~8 frames each) | Animated bushes |
| Rocks | 4 variants | 64x64 each | Static rock props |
| Water Rocks | 4 variants | Animated strips | Rocks sitting in water |
| Clouds | 8 variants | Various | Cloud shadows / overlays |
| Trees | 4 variants | ~192x256 (in 1536x256 strip, 8 frames each) | Animated trees (swaying) |
| Tree Stumps | 4 variants | Static | Cut tree stumps |

### Resources

| Resource | Assets | Description |
|----------|--------|-------------|
| Gold Stones | 6 variants (static + highlight animation) | Gold ore deposits |
| Gold Resource | 1 (static + highlight) | Refined gold pile |
| Wood Resource | 1 static sprite | Chopped wood pile |
| Sheep | Idle, Move, Grass animations | Animated livestock |
| Tools | 4 static sprites | Axe, hammer, pickaxe, knife on ground |

## UI Elements

### Bars (Health / Morale)

| File | Size | Description |
|------|:---:|-------------|
| `BigBar_Base.png` | 320x64 | Large bar frame (3-part: left cap, middle, right cap) |
| `BigBar_Fill.png` | 64x64 | Large bar fill (stretchable) |
| `SmallBar_Base.png` | 320x64 | Small bar frame |
| `SmallBar_Fill.png` | 64x64 | Small bar fill (stretchable) |

### Buttons

16 button variants in total:

- **Big** buttons: Blue and Red, Regular and Pressed states (320x320)
- **Small Square** buttons: Blue and Red, Regular and Pressed
- **Small Round** buttons: Blue and Red, Regular and Pressed
- **Tiny Square** buttons: Blue and Red (single state)
- **Tiny Round** buttons: Blue and Red (single state)

### Cursors

4 cursor variants (`Cursor_01.png` through `Cursor_04.png`) at 64x64 each.

### Human Avatars

25 portrait sprites (`Avatars_01.png` through `Avatars_25.png`) at 256x256 each. Useful for commander portraits, squad leader display, or death screen character identification.

### Icons

12 icon sprites (`Icon_01.png` through `Icon_12.png`) at 64x64 each. Suitable for ability buttons, status indicators, or menu items.

### Papers / Panels

| File | Description |
|------|-------------|
| `RegularPaper.png` | Parchment background for menus/dialogs |
| `SpecialPaper.png` | Decorated parchment for special screens |

### Ribbons and Banners

| File | Description |
|------|-------------|
| `BigRibbons.png` | Large decorative ribbon |
| `SmallRibbons.png` | Small decorative ribbon |
| `Banner.png` | Large banner (704x512) |
| `Banner_Slots.png` | Banner with item slots |

### Other

| File | Description |
|------|-------------|
| `Swords.png` | Crossed swords decoration |
| `WoodTable.png` | Wooden table background |
| `WoodTable_Slots.png` | Table with item/card slots |

### Faction Ribbons (Store Banners)

Colored ribbons for each faction (960x576 each): Black, Blue, Purple, Red, Yellow.

## Particle Effects

| Effect | Files | Frame size | Frames | Description |
|--------|:---:|:---:|:---:|-------------|
| Dust (small) | `Dust_01.png` | 64x64 | 8 | Movement dust puff |
| Dust (large) | `Dust_02.png` | 64x64 | 10 | Larger dust cloud |
| Explosion (small) | `Explosion_01.png` | 192x192 | 8 | Combat hit / impact |
| Explosion (large) | `Explosion_02.png` | 192x192 | 10 | Large impact / destruction |
| Fire (small) | `Fire_01.png` | 64x64 | 8 | Small flame |
| Fire (medium) | `Fire_02.png` | 64x64 | 10 | Medium flame |
| Fire (large) | `Fire_03.png` | 64x64 | 12 | Large flame |
| Water Splash | `Water Splash.png` | 192x192 | 9 | Splash effect for water |

## Implementation Notes

### Sprite Sheet Parsing

All animated sprites are **horizontal strips**. To extract frame N:

```
x = N * frame_width
y = 0
width = frame_width
height = frame_height
```

Frame width = total image width / number of frames.

### Facing Direction

Units face **right by default**. For left-facing:
- Mirror the sprite horizontally
- Exception: Lancer has explicit directional sprites (Up, UpRight, Right, DownRight, Down). Mirror for left-side equivalents (UpLeft, Left, DownLeft).

### Scale Consistency

- Terrain tiles: 64x64
- Unit frames: 192x192 (3x3 tiles) -- units are visually centered in their frame and occupy roughly 1-2 tiles of actual space
- Lancer frames: 320x320 (5x5 tiles) -- mounted unit needs more space
- The frame sizes include padding for animation movement (weapons swinging, etc.)

### Z-Ordering

The top-down perspective requires careful draw ordering:
1. Terrain tiles (bottom layer)
2. Decorations and resources
3. Building bases
4. Units (sorted by Y position, lower = drawn later)
5. Building tops (roofs that units walk behind)
6. Particle effects and UI (top layer)

### Available Faction Pairings for Battle

Any two of the 5 factions can be paired. Recommended defaults:
- **Blue vs Red** -- Classic, high-contrast matchup
- **Purple vs Yellow** -- Alternative high-contrast
- **Black vs any** -- Dark/evil army variant

### Missing Assets (Not in Free Pack)

The following would need to be created, sourced separately, or designed around:
- **No death/dying animation** -- Units can fade out, play explosion FX, or simply disappear
- **No damage/hit reaction** -- Use explosion particle FX on hit, or tint the sprite red briefly
- **No siege equipment** -- No catapults, battering rams, etc.
- **No terrain obstacles** -- No walls, fences, barricades (buildings can substitute)
- **No weather overlays** -- Rain, fog, snow would need custom implementation
- **No directional sprites for most units** -- Only Lancer has facing variants; others face right only (mirror for left)
- **No up/down facing** -- Warrior, Archer, Pawn, Monk only face left/right

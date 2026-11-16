# Actions & Beyond for Horizon

Actions & Stuff's animated mobs, players and first person arms on
[Horizon](https://playhorizon.dev). This native mod reimplements the Actions & Beyond 1.13 Fabric
port's Bedrock entity runtime in Rust: Molang, geometry, animations, animation and render
controllers, attachables. The models, animations, textures and sounds come from your own copy of
the mod, built into a content mod (`ans.hmod`) on your computer.

Version 1.0.0, for Minecraft 26.3, pinned to Horizon `9f42064c11efe7ef3e5ed2baad7ddc2a3b36eea0`
(Nightly 2026-11-13) and horizon-mixins `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`.

## What it does

- Every mob Actions & Stuff animates is drawn with its model, animations, sounds and variants,
  including composed textures (sheep wool in its dye colour).
- Players are drawn with A&B's model and your skin, including expressions, worn armour on A&B's
  bones and on morphs, held items in A&B's hands, and the pack's 3D held items.
- First person: A&B's arms and their animations replace vanilla's, with the pack's 3D items
  held in the animated hand.
- The pack's settings (morphs, armour visibility, holstered weapons and the rest) on Actions &
  Stuff's own settings screen (press K), plus this port's Viewmodel settings (arm size, height,
  side and depth, item size).
- An enchantment outline around enchanted held items, in the style of Enchantment Glint Outline:
  glint or solid colour, size, and whether the item keeps its own glint.

What it doesn't do (yet):
- Particles from the pack's animations aren't drawn.
- Additive and translucent layers (some glowing eyes) are left out; emissive layers draw full bright.
- A few queries answer 0 (biome, some camera queries).
- A solid colour outline on a dark item comes out darker than the colour picked: Horizon's item
  shader multiplies it by the item's texture. The glint style doesn't have this limit.

## Install

You need Horizon's launcher, Native mods turned on, and your own copy of the Actions & Beyond 1.13
Fabric jar (`ActionsAndStuff-1_13-port3.jar` or later).

1. **The native mod.** In the launcher's Mods page, "Add from GitHub" with this repository's link,
   or download `ans_beyond-1.0.0.hnmod` from its release and put it in the instance's
   `horizon-native-mods/` folder. Then build (Mods page, "I trust these mods: build").
2. **The content mod.** Build it from your jar (Python 3 and Pillow):

   ```
   python3 tools/build_content.py <the jar unpacked> <out folder>
   ```

   Zip the out folder's `horizon-mod.toml` and `assets/` into `ans.hmod` and put it in the
   instance's `horizon-mods/` folder. The native mod draws nothing without it.
3. Play, and turn on Actions & Beyond in Options > Mods.

The settings are also in `config/ans_beyond.txt` (written on first start; the game reads changes
within a second).

## Performance

The engine animates and meshes on the render thread. Measured with `engine/examples/bench.rs`
on one core, counted with callgrind (instructions per draw, steady state):

| Draw | Instructions |
| --- | --- |
| A zombie | 450k |
| A villager | 810k |
| A player, third person, holding a sword | 1.36M |
| The first person arms | 1.37M per frame |

On a typical desktop CPU that is roughly 0.05 to 0.15 ms per mob and 0.2 to 0.3 ms per player.
A mob type's data loads once, the first time one is drawn (1 to 20 ms; the player's 140 ms).

## Build and test

The engine is plain Rust with no dependencies (`mixins/ans_beyond/engine`). With your content mod
unpacked in `<hmod>`:

```
cd mixins/ans_beyond/engine
cargo test --release                                     # unit tests
ANS_DATA=<hmod> cargo test --release                     # also every Molang expression in the data
cargo run --release --example sweep -- <hmod>            # every held item, both hands, both views
cargo run --release --example allmobs -- <hmod> 1 idle   # every mob type
cargo run --release --example bench -- <hmod>            # what a draw costs
```

The native mod itself is a horizon-mixins bundle (`mixins/ans_beyond`): `horizon-mixins apply
--workspace <horizon at the pinned commit> --mods mixins --out <stage>`, then
`cargo check -p horizon-client` in the stage.

## Licence and credits

The code in this repository is MIT (`LICENSE`). It contains none of Actions & Stuff's assets:
those are Oreville Studios' and not licensed for redistribution, so `ans.hmod` is built from your
own jar and is not shared. See `CREDITS.md`, and `CHANGELOG.md` for every change.

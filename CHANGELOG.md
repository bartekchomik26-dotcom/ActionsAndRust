# Changelog

## 1.0.0 (2026-11-16)

The first release. It is horizon.23 plus:

- Faster: a mob's draw costs 26 to 43% fewer instructions than horizon.19 (callgrind,
  `examples/bench.rs`): zombie 634M to 360M per 800 draws, a player in third person 846M to
  543M, the first person view 207M to 137M, a villager 867M to 646M. The output is unchanged: the
  6,568 held-item sweep and all 117 mob types draw the same quads as before.
  - Render controller bone patterns (`*arm*`) are matched once per model, not every frame
    (`Geometry::matching`); the matcher itself no longer allocates.
  - The engine's maps use FxHash (`FastMap`) instead of SipHash: they are keyed by interned
    names and the engine's own strings.
  - Pack settings are looked up by id in a map instead of a list scan.
  - Last frame's bone pose is swapped with a spare buffer instead of cloned on every draw, and
    held items' setup scripts are no longer cloned each frame.
  - The game side asks for the composed textures' sprite sizes once, not on every mob's draw (again
    after a resource reload).
- `horizon-registry.toml` holds only the page fields Horizon's registry reads.

## horizon.23 (keep `ans.hmod` 1.13.0-horizon.20)

- Seen in game with horizon.22: a held elytra or chestplate drawn once; no stray
  arm while swimming or sprinting; armour on the slime morph (chest, arms, legs;
  its helmet does not show yet); the enchantment outline drawn round a held
  enchanted sword, though rough: thin fins along a flat item's edges and a dark
  band beside it.
- The outline cleaned up: a flat item's one pixel edge strips are no longer
  copied (their copies were the fins), and the copies sit only a hair off each
  face instead of the outline size (that lift moved the back face's copy sideways
  into the band). Not yet seen in game.
- Known limit: the game's item shader multiplies the outline colour by the item's
  texture, so a solid outline on a dark item (netherite) comes out darker than
  the colour picked. The Glint style does not have this problem.

## horizon.22 (keep `ans.hmod` 1.13.0-horizon.20)

- The other arm through the camera: swimming, gliding or sprinting poses swing
  the off-hand arm into the camera, where it filled the screen. A box of the
  first person model that the camera is in (within 0.05 blocks) is left out.
  Checked offline: swimming, the left arm's boxes reach the eye (distance 0)
  and the main arm stays at 0.12 or more.
- Armour on morphs: the port drew no armour on a morph. The jar's
  `EquipmentLayer` has no morph check: each piece goes on the morph's head, body,
  arm and leg bones (named with the pack's prefix, `egjh9head`...) and is
  stretched so the vanilla part's box lands on the bone's first box
  (`EquipmentLayer.fit`). The port now does the same, and the hands, pose and
  armour use the model that is drawn (the morph's), not the player's under it.
  Checked offline: all 13 morphs get armour, in their own proportions.
- The enchantment outline now works like Enchantment Glint Outline (CC0,
  Da0neDatGotAway/better-enchants): each face is copied four times, shifted along
  its diagonals and lifted along its normal (`QuadHelper.thickenQuad`). The
  default style is the glint on the outline, and the item's own glint moves onto
  the outline (the mod's `removeRenderPass`). Settings > Item outline: style
  (glint / solid), colour (solid; lilac 210 150 248 by default, as the mod),
  size, and "Shimmer on the item". In `ans_beyond.txt`: `enchant_outline_style`,
  `enchant_outline_color`, `enchant_outline_width`, `enchant_outline_item_glint`.
- Checked in the cloud: engine tests, the 6,568 held-item sweep identical to
  horizon.19's, offline renders of morph armour, and a Horizon compile check.
  Not yet seen in game.

## horizon.21 (keep `ans.hmod` 1.13.0-horizon.20)

- A chestplate, helmet, leggings, boots or elytra held in the main hand was drawn
  twice. Actions & Stuff's own player model draws a held armour piece as a 3D
  model (its render controllers `ryfpdk`, `ieaaxo`, `nnryoa`), and the port then
  drew the game's item as well, whatever the settings. When the model draws it,
  the game's item is now left out, in first and third person.
- The holes in the first person arm: every layer after the first on the player
  model was lifted a little (to avoid flicker) by pushing each face out along its
  normal, and the count included layers that draw nothing. The arm was pushed
  out by up to 0.03 blocks per face, which opened a crack along every edge. Now
  only layers that draw count, and a box grows evenly instead, so its faces still
  meet (a new test checks every corner is shared by three faces).
- Enchantment outline: an enchanted item held in hand gets a coloured outline with
  the glint over it, in first and third person: the pack's 3D items, its held
  armour pieces and the game's own items. Settings > Item outline:
  Enchantment outline (on/off), Outline colour (purple, blue, aqua, green,
  yellow, orange, red, pink, white, rainbow), Outline width. In
  `ans_beyond.txt`: `enchant_outline`, `enchant_outline_color` (0 to 9) and
  `enchant_outline_width`. The pack's own 3D items also keep the game's glint (h19).
- Checked in the cloud: engine tests, a sweep of all 6,568 held-item cases
  identical to horizon.19's, an offline first person arm with closed edges, and a
  Horizon compile check. Seen in game for horizon.20: sheep wool and a dyed sheep.
  Not yet seen in game: this build's fixes and the outline.

## horizon.20 (with `ans.hmod` 1.13.0-horizon.20: install both)

- Sheep look right: the jar builds a sheep's texture at load time from the game's
  sheep body, wool undercoat and wool, the wool parts in the sheep's wool colour
  (`BedrockTextures.composite`). The port drew the body texture alone. The engine
  now draws each part (`composites.json`, 19 recipes taken from the jar), so wool
  shows, dyed sheep keep their colour and `jeb_` cycles. Cats' tame babies (collar
  in its dye colour), drowned, the phantom, the warden's tendrils and the command
  block frames use the same recipes.
- Checked in the cloud: engine tests (a new composite test), an offline sheep draw
  (body 40 quads, undercoat 40 and wool 36, both in wool colour), Horizon compile
  check, and `ans.hmod` read by Horizon's content reader. Not yet seen in game.
- Not fixed yet, could not be reproduced offline: a chestplate or elytra held in
  hand drawn twice, the gaps in the first-person arm, and the enchantment glint.

## horizon.19 (with `ans.hmod` 1.13.0-horizon.19: install both)
- Enchanted items held as 3D models get the game's enchantment glint (the pack's own glint layers are additive,
  which this drawing can't blend), in both views.
- Flickering stripes on held models: a model's layers over the same geometry (its enchanted look, glow overlays)
  are now drawn a little apart, as the player's own layers already were.
- An item with no 3D model (an elytra or a chestplate in hand) is held as the jar's first person holds it, with
  the third person hand transform, and Item Size applies to it: it was drawn huge with the first person one.
- Glowing textures (the morphs Pixie, Iris, Rusty, Echo, Ash, Russell, glowing items): drawn as the jar draws
  them, the texels that don't glow lit by the world and only the glowing ones full bright (`<name>__lit`,
  `<name>__glow` halves); they were all full bright.
- `ans.hmod` was checked with Horizon's own content loader before delivery.

## horizon.18 (keep `ans.hmod` 1.13.0-horizon.17, the fixed one that declares `ans_ui`)
- horizon.17's first `ans.hmod` was refused by Horizon ("a mod adds files in its own namespaces"): the settings
  screen's `ans_ui` folder wasn't declared. The content manifest now lists `namespaces = ["ans_ui"]`; the file was
  checked with Horizon's own content loader (`horizon_mods::Content::read`) before delivery.
- Slimes and magma cubes were invisible: the pack scales them by `q.variant`, which for them is their size; the
  port answered the generic variant (0). It now answers the size (1, 2, 4).
- The black seam down the first person arm's edge: each face's texture coordinates now stay a twentieth of a
  texel inside the face, so an edge no longer samples the skin's next (often unused, black) texel.

## horizon.17 (with `ans.hmod` 1.13.0-horizon.17: install both)
- The content mod is built from the newer Actions & Beyond 1.13 port (port3 jar): about 2,170 updated models,
  textures and attachables, and the new axolotl bucket colour option. The offline sweep of all 1,698 held items in
  both hands and both views found no new problems.
- The settings screen (K) now looks like the jar's own hub: a sidebar of categories (General, Visuals, Item outline,
  Animations, Sounds, Viewmodel), each page's title picture, rows with the jar's icons, titles and descriptions in
  its Minecraft Ten font, sliding orange switches and value steppers (click the left or right half), a preview with
  your skin, the Nether menu colours and the jar's interface sounds. The port's arm and item settings are under
  Viewmodel. The port's two Java-only settings (biome sky colours, particles) are not shown.
- Black flashes in first person:
  - Inside-out cubes (1,527 cubes with negative sizes, e.g. the ender pearl's glass shells around its black core,
    potions and bottles) were drawn facing outward as solid boxes; they now face inward as Bedrock draws them.
  - A held model the camera ends up inside (a thrown item's swing brings it to the face) is left out for those
    frames instead of covering the screen.
- Glowing textures follow the jar's rule (`BedrockTextures`): only texels with some alpha are drawn, and a texture
  with no glowing texels is lit normally instead of full bright.

## horizon.16 (with `ans.hmod` 1.13.0-horizon.16: install both)
Every one of the 1,698 items with a 3D model was held in each hand, in first and third person, by an offline sweep
(`engine/examples/sweep.rs`), and what each drew was checked against its textures. Fixed from it:
- 271 materials of types the engine left out (`emissive_opaque`, `emissive_opaque_cull`, `emissive_alpha_cull`) are drawn:
  the diamond, emerald, ingots, nuggets and many glowing items drew nothing or only parts.
- A render controller without `materials` uses the bundle's default material, as Bedrock does (it was always cutout):
  the redstone block, glowstone, sea lantern, blaze rod, shroomlight, end rod, spawn eggs and golden apple were drawn
  from their glow mask and showed nothing.
- See-through items (glass and panes, potions, the totem, honey and dragon's breath bottles, keys, the fungi, discs):
  the pack's translucent item material is drawn as cutout instead of left out. Offhand totem now draws.
- Glow textures: an opaque copy (`<name>__opaque.png`) is shipped for every texture a glowing material draws, including
  ones picked from texture arrays (the experience bottle), and only when some texel would otherwise be dropped.
- The 3D blaze powder and the redstone block (its texture is composed by the jar at run time: its cells now point at the
  game's own block and item sprites, `atlas_cells.json`).
- The 1024-row textures are cut in overlapping 512-row parts (one every 256 rows), so a quad near a cut keeps its own
  texels instead of sampling the next sprite in the atlas (the campfire, glow item frame, pottery sherd, blaze powder).
- `fp_item_scale` (new, default 0.8; Item Scale in the K screen): first person size of 3D held items, about the hand.
- The black face for a moment: the pack's eyes and blinks read their pixels from parts of the skin a skin normally
  leaves empty, which the game makes opaque (black) when it loads a skin. As the jar does (`ExpressionSkins`),
  expressions are now on only for a skin whose original file has at least two opaque colours there.
- The K screen's gold line stops above the description area.
- Not drawn by the pack, on purpose: armour, harnesses, the elytra, saddles and wolf armour in a hand (models made to be
  worn), and the filled map (the game draws its map). These show as the game's own held item.

## horizon.15 (keep `ans.hmod` 1.13.0-horizon.14)
- The Actions & Stuff screen: press **K** in game (`menu_key` in `ans_beyond.txt`). Black and gold; your skin and the
  morph on the left, a gold line, the settings on the right: Arm Scale, Arm Height / Side / Depth, 3D Tools, 3D Held
  Items, Held Items, Items Follow Arm, Worn Armour, then every Actions & Stuff setting with the pack's own names and
  descriptions (hover a row to read it). Left click steps forward, right click back; the wheel scrolls; Reset and Done
  at the bottom; Escape or K closes it. Every change is written to `config/ans_beyond.txt` straight away.
- `fp_arm_scale` (new, 1 = the jar's): first person arm size, about the shoulders.
- Holding an armour piece or the elytra no longer draws the worn model in front of the camera: an attachable that binds
  none of its bones (made to be worn) isn't used in a hand, the game's own item is drawn there. Worn, the elytra is
  still the jar's.

## horizon.14 (with `ans.hmod` 1.13.0-horizon.14: install both)
- Held items draw as the jar's 3D models with their own hand animations, in both views. Found in game with the debug
  trace: the 3D items (and the stick) sample A&B's 1024x1024 item texture `attachable/czc.png`, and Horizon's atlas
  skips any texture taller than 512 rows, so those models drew nothing (horizon.13 then fell back to the game's flat
  item, which doesn't follow A&B's hand poses). The content mod now ships the 7 textures that are too tall cut in
  512-row parts (`<name>__p<i>.png`, listed in `split_textures.json`), and the engine points each quad at its part.
- The elytra: drawn by the jar's attachable for the chest slot (its bones follow the player's of the same name, as
  `AttachableRenderer` does when an attachable has no `bone_bindings`); the player's worn armour now answers
  `q.is_item_name_any('slot.armor.*', ...)` in both views (it read as empty).
- Gliding: the body tips as the jar's `PlayerRenderer.applyRotations` does (`fallFlyingScale * (-90 - xRot)`).
- A&B's player engine variables are set as `PlayerQueries.engineVariables` sets them: `v.is_holding_right`,
  `v.is_holding_left` (read by 1,630 attachables), `v.player_x_rotation`, `v.gliding_speed_value`,
  `v.is_holding_spyglass`, `v.goiapv` (main hand item has foil), `v.is_tooting_goat_horn`, `v.is_using_brush`.
- A bucket's water (and other `entity_alphablend` parts) draws as cutout instead of not at all: Horizon's mob drawing
  can't blend, so it is opaque rather than see-through.
- Checked in game on horizon.13: standing still no longer plays the flying pose; walking animates.
- Type-checked against Horizon 9f42064 with horizon-mixins before shipping (`cargo check -p horizon-client`).

## horizon.13
- Builds: horizon.12 failed to compile (`Rc<str>::as_str` is unstable). This build was applied with horizon-mixins and
  type-checked against Horizon 9f42064 before shipping.
- A held item whose attachable uses a texture the game's atlas lacks no longer vanishes: that attachable is dropped and
  the game's own item is drawn at A&B's hand.
- `ans.hmod` is unchanged (still 1.13.0-horizon.12 content); keep the one from h12.

## horizon.12
- The `ans_debug` trace also answers a stick named with the quotes kept (`/give @s stick[custom_name='"ans_debug"']`
  names it `"ans_debug"` in this game); `/give @s stick[custom_name="ans_debug"]` works too.
- `ans.hmod` rebuilt from the jar as 1.13.0-horizon.12 (same content as horizon.8's: the entity bundles and the 1,698
  attachables).
- Everything listed under horizon.11 below is in this build; the horizon.11 you built first had only its first person
  part, not the ground and sprint fix.

## horizon.11
- First person with an item in hand, as the jar draws it (`FirstPerson.javaHands`): A&B shows its first person arm
  only while an attachable holds that item (the attachable's `parent_setup` sets `v.oreville_mainhand_visible`), so
  **the arm with items needs the horizon.8 `ans.hmod` or newer** (the horizon.1 content has no attachables, and there
  the jar shows no arm either). An item without an attachable stays where vanilla holds it.
- Vanilla's first person geometry is now sorted hand by hand: before, without a shader pack every run had id 0 and the
  held items were thrown away with vanilla's arm, so a held item could show no hand and no item at all.
- Third person, your own player: standing on the ground no longer plays the falling/flying pose, and sprinting and
  jumping play. Horizon's model of the window's own player carries no ground, sprint or jump flags (they were always
  off), so these now come from the player itself. First person reads the real ground flag too.
- The `ans_debug` trace also logs the ground and sprint flags, the sprites the game's atlas lacks, and for each held
  item's attachable: its geometry, texture, the hand bone it is bound to, how many quads it drew and where.
- `first_person_items_follow_arm` is off by default (the jar's viewmodel is off by default). The txt is rewritten with
  the new defaults on first start (config_version 5).

## horizon.10 (unverified in game)
- Your own player in third person is found by the game's own flag (`NativePlayerRender.local`), not by matching ids.
- A trace for testing: hold an item named `ans_debug` (`/give @s stick[custom_name='"ans_debug"']`) and the game log
  gets A&B's walk and turn variables twice a second, in each view.

## horizon.9 (unverified in game)
- Pinned to Horizon nightly 2026-11-13 (`9f42064`). Horizon moved the first person hand function
  (`hand_geometry`) to `game/frame_geometry.rs`; the hand injection follows it. Every other hook and API the mod uses
  was checked against the new source and is unchanged.
- Walking: `q.position` is answered in first person too (the A&B speed script reads it), so the first person walk sway
  runs.
- Held items sit where the jar puts them: a bound bone follows the hand bone's pivot as `AttachableRenderer.anchors`
  does (a bone with a parent uses its parent's pivot; one without sits 1.5 blocks lower). Shields and crossbows were
  floating away from the hand before.
- An item's second texture is drawn only for materials marked `multi_texture`; the others are enchantment glint masks
  (black when not enchanted), which were drawn over items. Enchanted items show no glint yet.
- First person: the arm sits 0.2 blocks higher and 0.12 further in than the jar puts it (`fp_arm_offset_*` in the
  txt; 0 0 0 is the jar exactly, which leaves the empty arm mostly off screen). The txt is rewritten with the new
  defaults on first start (config_version 4).
- `previews/`: GIFs and stills of the bundle in third and first person (see previews/README.md).

## horizon.8 (needs the new ans.hmod)
- Held items drawn as the jar draws them: its 1698 attachables (3D items: the axolotl in its bucket, 3D swords and
  tools...), run on the holder's variables and bound to its hand bones. Their `parent_setup` scripts are what show the
  first person arm while holding an item, so the arm now appears with items too. `attachables = false` in the txt turns
  them off (vanilla items in A&B's hands instead).
- The content mod carries the attachables (models, animations, textures), so update ans.hmod with this build.

## horizon.7 (unverified in game)
- Fixed `q.any` / `q.all` comparing strings as numbers (every string was 0, so `q.any(v.x,'a','b')` was true for
  any string): this raised the right arm with empty hands, among other things.
- Worn armour drawn on A&B's model, posed with it (vanilla's humanoid armour, as the jar's EquipmentLayer); off with
  `armor = false` or the pack's armour visibility setting, hidden while morphed.

## horizon.6 (unverified in game)
- Walking and the other movement animations: A&B's own `q.ans_tick_position` (the jar's tick position the movement
  scripts take speed and direction from) and `q.bone_orientation_trs` (leg and foot placement) are answered now; both
  were 0 before, so walking did not match the jar.
- Holstered weapons are no longer black (a mask texture was drawn over them); the held item points forward.
- First person: A&B's arms raised into view (`fp_arm_offset_*` in the txt) and items held in their hand.
- `q.time_of_day`, `q.ans_arm_inertia`, `q.ans_outline_color` answered.

## horizon.4 (unverified in game)
- Third person: camera, head and body queries answered (the model was turned round and its scripts gave NaN), swing
  (`attack_time`) and bob passed in, the hotbar read for holstered weapons (trident / sword on the back), crossbow
  charge, item use time, blocking. A bone a script breaks stays at rest instead of losing the model.
- Multiselect pack settings (holstered weapons, special movement animations) now start with all their defaults on.
- Held items drawn in A&B's hands (`rightitem` / `leftitem` bones) in third person, and in first person they follow the arm.
- `config/ans_beyond.txt` (written on first start): held items on/off, item angle and offset, and every pack setting
  (morph = axolotl, enchant look, armour shimmer, expressions, 3D tools...). Saved changes are picked up within a second.

## What works (engine checked in the cloud, not yet in game)
- Molang: all 214,592 expressions in the jar compile.
- Animations, animation controllers (with blend curves), timelines, render controllers (geometry, texture, part visibility, materials, hurt/overlay colours, uv_anim).
- All 116 entity bundles load, animate and mesh.
- Mob models drawn through `native_mob_submit`. Sounds from animations are played through `play_local_sound`.

## Not yet / limits
- Players: A&B's player model and animations with each player's skin, including your own in third person (F5).
- First person: A&B's arms, posed by its first-person animations (swing, walk, sneak and so on), replace vanilla's arm in the hand pass. Held items are still vanilla's and don't follow the arm yet.
- Particles: not drawn. Horizon only has a flame-particle helper.
- Additive and translucent layers (glow eyes and the like) are left out. Emissive layers are drawn full bright.
- About 20 queries return 0 for now, including biome, rotation_to_camera, bone_orientation_trs and camera distance. Sneaking, sprinting, gliding, swimming, sitting, tame, saddled, angry, target and riding are read from the game.

## Cost (cloud benchmark, one core)
About 0.02–0.06 ms per mob per frame to animate, and 0.17 ms for a player. Data for a mob type loads once, in 1–20 ms (the player bundle takes 140 ms).
- The `ans_textures` / `ans_colors` resource packs (vanilla overrides) are not included yet.

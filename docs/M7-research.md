# M7 research: items on machines

Goal of M7 (from the [roadmap](../ROADMAP.md)): a **Depot** that holds one item, a **Mechanical
Press** over it, and a **Millstone** that grinds items, all driven by the kinetic network.

This note records what the pinned SDK (Horizon `0b14f8a`) offers, what is still unknown, and a way
to build it in steps. Everything below was read from the SDK sources under `.horizon/sdk`; nothing
here has been run in the game yet.

## What the SDK gives us

| Need | API | Notes |
| --- | --- | --- |
| Item slots on a block | `Registrar::block_container(&block, ModContainer { slots, faces, inputs, outputs, menu })` | For **mod blocks** only. `faces` lists the slots each side reaches; `inputs` are the slots a hopper may put into, `outputs` those it may take from. Hoppers, comparators and breaking the block (items drop) then work with no code of ours. `menu: None` means no screen. |
| Block entity to hold them | `registrar.block_entity_type("depot").blocks(&[...]).build()` | The same call the mod already uses for `kinetic`. The container belongs to the **block**, so the depot, press and millstone need block entity types of their own (or one shared type listing all three). |
| Read and write slots | `ModBlockEntity` `item_slot(host, slot)` and `set_item_slot(host, tx, slot, Option<ItemStack>)` | Needs a `Transaction`. Slots live in the entity's saved data, so they survive restarts. `block_entity_tick` also receives the slots. |
| Move items between blocks | `level.capability(host, &ITEMS, pos, side)` and `move_resources(host, &from, &to, filter, max, tx)` | The way a pipe or a press would take from the depot below it, by the same rules hoppers follow. |
| Put an item in or take it out by hand | `UseItemOnEvent` (already used for the wrench) and `UseWithoutItemEvent`; `player.set_hand_item(host, Hand, Option<ItemStack>)`, `player.give_stack(host, stack)` | The click carries the held stack. |
| Make items appear | `level.spawn_item(host, x, y, z, stack, pickup_delay)` and `spawn_item_with_motion(...)` | For machine outputs thrown out of a millstone, or dropped at the end of a press. |
| Find loose items | `level.entities_in(host, min, max)` returns `EntityRef`s | Needed only if a machine should work on dropped items (Create's depot does not; a slot is simpler). |
| Recipes | `host.recipes_of_type("create:pressing")` gives each recipe in force as a `Recipe` with `id`, typed `ingredients` and `results`, and its file's `json`. Custom types are plain data files in `data/create/recipe/`. | `level.recipe_for` only matches vanilla types (a custom type returns `NotMatchable`), so cache `recipes_of_type` once per load and `/reload` and match in memory. Vanilla's crafting, smelting and stonecutting recipes can also be read the same way. |
| Draw the item on the depot | `BlockEntityDrawContext::item(stack, ItemDisplayContext, owner_yaw, seed)` inside `block_entity_submit`, plus `spin(axis, pivot, degrees_per_tick, phase)` | The client's copy of the entity has **no item slots**: send the stack in the `block_entity_update_tag` hook (and `set_client_update_fields` for live changes) and read it from `entity.nbt` when drawing. |

`spin` is also a cheaper way to turn kinetic blocks than the per-frame angle the mod computes now
(the game turns the part by itself between updates). Worth switching to when M7 is done; not a
requirement.

## What M7a settled (tried in the game)

1. **Transactions:** `host.transaction(|host, tx| ...)` (the `HostTransaction` trait) works inside
   `use_item_on` and `use_without_item`; `entity.set_item_slot(host, tx, slot, stack)` then
   `entity.set_changed(host)` writes the slot. The block entity comes from
   `host.block_entity(level, pos)`.
3. **Syncing the item to clients:** `set_client_update_fields` alone did **not** reach the client
   until its chunk was sent again. What works: save the stack on show with
   `entity.merge_data(host, {Shown: stack.to_nbt(host)})`, then `level.send_block_updated(host,
   pos)`, and send it in `block_entity_update_tag` for chunk loads. The tick does the same for
   changes made by hoppers. An empty compound under the same key tells clients the item is gone.
4. **Hoppers:** a hopper below pulls the item out of the depot and a hopper above fills it, with
   nothing but the declared `ModContainer`. A hopper puts a whole stack in; the depot keeps it.

Also learned: an item stack's id cannot be written without a host (`to_nbt(host)`), so the stack
on show has to be serialized where a host exists (a click or the tick), not in the update-tag hook.

## What is still unknown (to test before relying on it)

2. **A custom recipe type with no declaration.** The notes say a type nobody uses "gives nothing"
   and a mod's own type has only its `json`, which suggests no registration is needed. To be
   confirmed with one `create:pressing` file that loads without an error and appears in
   `recipes_of_type`.
5. **Which side a press takes from.** The depot below it is read through the `ITEMS` capability with
   `side = Up`, but the container's `faces` must then include the top.

None of these is a design risk; each is an hour of trial in the game.

## Proposed design

**Depot** (block, one slot).
- Right click with an item: put one item in if the slot is empty. Empty-hand right click: take it
  back (to the player, or dropped if the inventory is full). Hoppers fill and empty it through the
  container sides.
- Draws its item on top, flat, with `Fixed` display.
- Not kinetic.

**Mechanical Press** (block, kinetic consumer, one input from the block below).
- Takes the item of the block entity directly below (a depot or any container with a `Some(Up)`
  face), and works while it turns, as the drill's progress counter does: an `RPM`-ticks job from
  the `create:pressing` recipe. When done, it replaces the item in that slot with the recipe's
  result.
- Draws a ram that goes down and up with the progress.
- Impact: 8 stress units per RPM (to be tuned).

**Millstone** (block, kinetic consumer, one input slot and one output slot).
- Input from above (hoppers, or right click with an item), output from below and the sides
  (hoppers) and by empty-hand right click.
- Works while it turns on a `create:milling` recipe (for example cobblestone to gravel, bone to
  bone meal, wheat to flour if flour exists; the starter set is the vanilla-only pairs). The result
  goes to the output slot; what does not fit stays, and the millstone waits.
- Draws two turning stones.
- Impact: 4 stress units per RPM.

**Recipes** are data files, so packs can add to them:

```json
{
  "type": "create:pressing",
  "ingredient": "minecraft:iron_ingot",
  "result": { "id": "minecraft:iron_nugget", "count": 9 },
  "work": 400
}
```

A pure module (`src/processing.rs`) holds the matching and the progress arithmetic, with unit
tests, exactly as `work.rs` does for the existing machines. The adapter only reads the world.

## Suggested build order

Each step ends with a test in the game, as with M4 and M5:

1. **M7a, Depot.** One block, the container, right click in and out, a hopper above and below, the
   item drawn on top. Answers unknowns 1, 3 and 4.
2. **M7b, Press.** The press block, one recipe file, the progress and the ram. Answers 2 and 5.
3. **M7c, Millstone.** Two slots, a recipe file, outputs by hopper and by hand.

Estimated size: M7a a couple of hundred lines plus assets; M7b and M7c a little more each, mostly
because of the drawing. Compared with M4 and M5 the risk is higher in the first step only; after
the depot works, the rest is the same pattern as the drill.

## Out of scope for M7

Belts, chutes and funnels (M8), filters, fluids, and recipes with several inputs or random
outputs.

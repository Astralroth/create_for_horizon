# M8 research: logistics (belts, chutes, funnels)

Goal of M8 (from the [roadmap](../ROADMAP.md)): move items between machines. A **Belt** carries
items along a line, a **Chute** drops them down and a **Funnel** puts them onto a belt or takes
them off it. Everything here was read from the pinned SDK (Horizon `0b14f8a`); nothing has been run
in the game yet. M7 already settled how items are held, moved between containers and drawn (see
[M7-research.md](M7-research.md)); M8 adds items that **travel**.

## The central choice: how an item rides a belt

| Option | How it works | For | Against |
| --- | --- | --- | --- |
| **A. Item entities** | Each belt block pushes the dropped items above it with `entity.set_velocity` every tick; they land on the next belt. | Very little code; the game's own physics, drawing and pickup. | One entity per item (lag), items jitter and bounce, players pick them up unless `set_never_pickup`, hard to hand an item to a container cleanly. |
| **B. Items kept in the belt (chosen)** | The belt's block entity keeps a short list of items with a position along the belt; the server moves them and the client draws them. | Exact control, no entities, items go into containers through the `ITEMS` capability, cheap to save and to sync. | More code: the list, hand-offs, drawing. |

Create itself works the second way. Option B is the recommendation.

## What the SDK gives us

| Need | API | Notes |
| --- | --- | --- |
| A belt answers "put an item in" from hoppers, chutes and funnels | `registrar.block_capability(&ITEMS, &belt, \|host, at, state, entity\| Some(Box::new(BeltItems)))`, with a `ResourceHandler<ItemResource>` of the mod's own | The handler's `insert` and `extract` run inside a `Transaction` and must write the belt's block entity through it, so an abort puts it back. This is the one new piece of SDK use in M8. |
| Hand an item to what is in front of the belt | `level.capability(host, &ITEMS, next, side)` then `insert` in a `host.transaction` | Already used by the press. Works for another belt (through its own provider) and for any container. |
| Move items between containers on a timer | `move_resources(host, &from, &to, filter, max, tx)` | The core of a chute and a funnel. |
| The list of items in a belt | The block entity's saved compound (`event.entity.nbt`) in `block_entity_tick` | Items are saved with the chunk for free. |
| Tell the clients | `entity.merge_data` plus `level.send_block_updated` at each *change* (an item enters, leaves, or stops) | M7a learned that `set_client_update_fields` alone is not enough. |
| Draw the belt moving | `BlockEntityDrawContext::quad(sprite, corners, uv, tint)` with a UV offset taken from `animation_time()` and the signed speed | A scrolling texture needs no per-tick sync. |
| Draw the items | `BlockEntityDrawContext::item(stack, display, ...)` after `translate` to the item's place | As the depot does. |
| Carry players and mobs (optional) | `level.entities_in(min, max)` and `entity.set_velocity` / `entity.velocity` | Server side, in the belt's tick. Whether a *player's* velocity set on the server is honoured is untested. |

Not needed: custom entities, mixins, new capabilities in `horizon-mod.toml`.

## Design (option B)

**Belt** (block, kinetic consumer).
- A straight flat piece with a `facing` (one of the four horizontal directions); one block is one
  piece. Chains are made by placing pieces end to end; a turn is a piece whose front meets the
  side of another.
- Stress: a small impact per RPM (to tune). The items move at `speed * k` blocks per tick, `k`
  chosen so 16 RPM is about 2 blocks per second; direction follows the sign of the speed.
- The block entity keeps up to four items, each `{stack, position along the piece (0 to 1),
  tick it was last synced, moving or stopped}`. Items keep a gap so they do not overlap.
- At the end of the piece an item goes to the next belt, or into the container in front, or waits
  (stopped) when nothing takes it. A stopped item makes the ones behind it stop.
- Accepts items from hoppers, chutes and funnels through its `ITEMS` provider, and from a player's
  right click with an item.
- Clients draw the moving surface and the items from the last synced position plus
  `speed * elapsed ticks`, so nothing is sent each tick, only when an item enters, leaves or stops.

**Chute** (block, one slot): every few ticks it passes its item to the block below through
`ITEMS` (side up) and takes items dropped into it from above or by a funnel. A vertical hop, with
a short pipe model.

**Funnel** (block, faces a container or a belt): every few ticks `move_resources` takes one item
from the container it is attached to and puts it on the belt or container in front, or the other
way round when set to take items off a belt. Filters are out of scope for the first version.

**Carrying entities**: only after the above works (see the build order).

## What is still unknown (to test first)

1. **A custom `ResourceHandler` through `block_capability`**: a hopper above a belt must be able
   to insert, and the hand-off from one belt to the next goes through the same door. The trait is
   documented, but nothing in the mod has implemented one.
2. **Writing the belt's compound inside a handler's transaction** so an abort undoes it, as the
   trait requires.
3. **Smoothness of the extrapolated drawing**: whether the client's `animation_time` and the
   server's game time stay close enough that an item does not visibly snap when a sync arrives.
4. **A scrolling quad on a block entity**: `quad` with a changing UV every frame, lit correctly.
5. **Cost of ticking every belt every tick.** Belts with no items can skip their work quickly; how
   many belts a world can have before it shows is a number to measure, not guess.
6. **Players on the belt** (optional): whether `set_velocity` on a standing player moves them.

## Suggested build order

Each step ends with a test in the game, as with M7:

1. **M8a, one belt.** The block, the items list, the `ITEMS` provider, the moving drawing, a
   hand-off into a container in front, and a click to put an item on. Answers 1 to 4.
2. **M8b, belt chains.** Belt to belt, turns, a stopped item holding the line behind it, the
   speed and direction following the network. Answers 5.
3. **M8c, chute and funnel.** Two small blocks over `move_resources`.
4. **M8d, carrying entities** (optional). Answers 6.

Size: M8a is the largest single step so far (the list, the handler, the drawing), roughly twice
the depot; the rest repeat patterns already in the mod. Risk: medium to high for M8a, then medium.

## Out of scope for M8

Sloped and curved belt models, belts several blocks wide, filters and priorities, items on both
lanes of a belt, tunnels and brass funnels, and anything that needs the original mod's contraption
system.

## Answers from M8a

A custom `ResourceHandler` through `block_capability` works (hoppers fed the belt); writing the
belt data inside the handler works (`merge_data`); a scrolling quad works; the drawing extrapolated
from game time looked smooth. Still open: many ticking belts, turns, entities.

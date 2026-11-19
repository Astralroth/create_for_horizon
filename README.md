# Create for Horizon

Rotating machines for [Horizon](https://github.com/horizon-rust/horizon): a motor turns
shafts and cogwheels, gears change speed and direction, and a drill uses the power to break
blocks. This is an independent project, written from scratch as a native Horizon mod and
inspired by the gameplay of the Create mod. It contains no code or assets from Create or from
other ports.

**Status: MVP.** The whole loop works in the tests and passes Horizon's own package checks.
Parts of it, listed under [Limitations](#limitations), have not been tried in the running game.

## Install

In Horizon Launcher (0.2.70 or later), open **Mods → Add from GitHub**, paste
`https://github.com/Astralroth/create_for_horizon`, and build the instance. The mod is pinned to
Horizon `0b14f8a` (nightly 2026-11-18) for Minecraft 26.3. Everything is in the **Create for
Horizon** creative tab.

## What is in it

| Item | What it does |
| --- | --- |
| **Creative Motor** | A source that turns at 16 RPM and carries 1024 stress units per RPM. |
| **Shaft** | Carries rotation in a line, at the same speed and direction. |
| **Cogwheel** / **Large Cogwheel** | Mesh with other cogwheels to change direction and speed. |
| **Mechanical Drill** | Uses 8 stress units per RPM and breaks the block in front of it. |
| **Wrench** | Right click a machine to turn it a quarter turn about the face you click. |

Recipes: Shaft = iron nugget + stick (makes 2). Cogwheel = shaft + planks. Large Cogwheel =
shaft + 2 planks. Wrench = 2 iron ingots + stick. Mechanical Drill = cogwheel + iron pickaxe +
iron ingot. The Creative Motor has no recipe.

### How to use it

* Place a Creative Motor and a line of shafts along its axis. Blocks turn after a moment.
* **Right click with an empty hand** on any machine to read its network: how many blocks are
  connected, the speed of that block, and the stress against the capacity.
* **Right click with the Wrench** to turn a block. A face on another axis moves the block to the
  third axis; a face on its own axis steps it x, y, z. A drill turns its facing instead.
* The drill faces where you look when you place it, and works while the network turns it.

## How the network works

Every block turns about one axis and has a shaft end at both ends of it. Blocks join in two ways:

* **Shaft link**: neighbours along the shared axis keep the same speed and direction.
* **Mesh**: two cogwheels on the same axis. Same size, edge to edge: same speed, opposite
  direction. A small and a large cog, corner to corner: opposite direction, and the large one
  turns at half the speed.

A network **locks** when its sources or its gear loops ask for speeds that cannot all hold, and it
**stops** when the machines use more stress than the sources carry. A machine uses its impact
times its speed, so gearing a machine down also lowers its stress.

## Develop

The kinetic solver (`src/engine.rs`), the direction rules (`src/direction.rs`) and the display
helpers (`src/visual.rs`) are plain Rust with no game types and have unit tests. `src/lib.rs`
connects them to the game.

```sh
cargo horizon-mod check        # validates the package against the pinned SDK
cargo horizon-mod build        # writes target/horizon-mod/create.hmod
cargo test --lib               # unit tests
```

`cargo horizon-mod` needs the SDK it fetches into `.horizon/` and a Rust toolchain for the pinned
build, with the `wasm32-unknown-unknown` target.

## Limitations

* **Seen only in tests and package checks.** Whether the blocks draw and turn in the running game,
  and the exact look of the models, have not been confirmed for the drill and the latest changes.
* Speed on screen is shown as the nearest power of two from 1 to 256 RPM; the exact value is in the
  click report. Blocks look at their network every half second.
* The Creative Motor has a fixed speed, and no way to change it yet.
* Cogwheels do not yet have a gearbox, clutch or belt; networks use straight shafts and cogs.
* The drill breaks any breakable block in front of it, containers included, and drops the items.
* Models use vanilla textures; the mod has no textures of its own yet.

## License

MIT, see [LICENSE](LICENSE).

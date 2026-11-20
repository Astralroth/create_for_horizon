# Create for Horizon

Rotating machines for [Horizon](https://github.com/horizon-rust/horizon): a motor turns
shafts and cogwheels (a hand crank and a water wheel make power too), gears and gearboxes change
speed and direction, a clutch and a gearshift cut and reverse the line with redstone, gauges show
what is going on, and a drill, a saw, a harvester and a plough use the power on the blocks in front
of them. This is an independent project, written from scratch as a native Horizon mod and inspired
by the gameplay of the Create mod. It contains no code or assets from Create or from other ports.

**Status: M0 to M6 of the [roadmap](ROADMAP.md).** Everything passes Horizon's own package
checks and the unit tests. M0 to M3 were tried in the game, and M6's look was seen there. The water
wheel (with the Wrench reversing it), the saw, the harvester and the plough were tried in the game
through Agent Control and worked; the hand crank charged and wore off as designed, but its link to
the shafts was fixed afterwards and is waiting for a second try. See [Limitations](#limitations).

## Install

In Horizon Launcher (0.2.70 or later), open **Mods → Add from GitHub**, paste
`https://github.com/Astralroth/create_for_horizon`, and build the instance. The mod is pinned to
Horizon `0b14f8a` (nightly 2026-11-18) for Minecraft 26.3. Everything is in the **Create for
Horizon** creative tab.

## What is in it

| Item | What it does |
| --- | --- |
| **Creative Motor** | A source. Sneak with an empty hand to step its speed 1, 2, 4 ... 256 RPM; the Wrench on an end of the motor reverses it. It carries 1024 stress units per RPM. |
| **Shaft** | Carries rotation in a line, at the same speed and direction. |
| **Cogwheel** / **Large Cogwheel** | Mesh with other cogwheels to change direction and speed. |
| **Gearbox** | Four shaft ends across its axis: turns a corner. A shaft straight through it comes out reversed. |
| **Clutch** | A shaft that lets go while it has a redstone signal, cutting the line in two. |
| **Gearshift** | A shaft that reverses the far side while it has a redstone signal. |
| **Speedometer** / **Stressometer** | Stick one on a block: a needle shows its speed, or the share of stress in use. |
| **Hand Crank** | A small source. Right click it with an empty hand: it turns at 32 RPM for a moment, and holding the use button keeps it turning. Sneak with an empty hand to read its network. It carries 256 stress units per RPM. |
| **Water Wheel** | A small source. It turns at 4 RPM for each of its four sides (across its axis) that has flowing water against it, up to 16 RPM; a still source block does not count. The Wrench on an end reverses it. 256 stress units per RPM. |
| **Mechanical Drill** | Uses 8 stress units per RPM and breaks the block in front of it. |
| **Mechanical Saw** | Uses 12 stress units per RPM and fells the log in front of it (any log, stem or wood block), a little faster than the drill would. |
| **Mechanical Harvester** | Uses 6 stress units per RPM. Harvests the ripe crop in front of it (wheat, carrots, potatoes, beetroots, nether wart) and plants it again. |
| **Mechanical Plough** | Uses 6 stress units per RPM. Turns the dirt, grass block or path in front of it into farmland when there is room above. |
| **Wrench** | Right click a machine to turn it a quarter turn about the face you click. |

Recipes: Shaft = iron nugget + stick (makes 2). Cogwheel = shaft + planks. Large Cogwheel = shaft +
2 planks. Gearbox = 2 cogwheels + iron ingot. Clutch = shaft + redstone + iron ingot. Gearshift =
shaft + redstone + gold ingot. Speedometer = cogwheel + compass. Stressometer = cogwheel + clock.
Wrench = 2 iron ingots + stick. Mechanical Drill = cogwheel + iron pickaxe + iron ingot. Mechanical
Saw = cogwheel + iron axe + iron ingot. Mechanical Harvester = cogwheel + shears + iron ingot.
Mechanical Plough = cogwheel + iron hoe + iron ingot. Hand Crank = shaft + 2 sticks. Water Wheel =
cogwheel + 4 oak planks. The Creative Motor has no recipe.

### How to use it

* Place a Creative Motor and a line of shafts along its axis. Blocks turn after a moment.
* **Right click with an empty hand** on any machine to read its network: how many blocks are
  connected, the speed of that block, and the stress against the capacity. On a gauge it reads the
  block the gauge is stuck to.
* **Sneak and right click with an empty hand** on the motor to step its speed.
* **Right click with the Wrench** to turn a block. A face on another axis moves the block to the
  third axis; a face on its own axis steps it x, y, z. On the motor, a face on its own axis reverses
  it instead (so does the water wheel). A drill, saw, harvester, plough or crank turns its facing.
* Gauges and the hand crank face out of the block they are placed against. The drill, saw,
  harvester and plough face where you look.
* Power a clutch or a gearshift with redstone (a lever next to it is enough).

## How the network works

Every block turns about one axis and has a shaft end at both ends of it, except the gearbox,
which has them on the four faces across its axis. Blocks join in two ways:

* **Shaft link**: two shaft ends facing each other. Along one line the speed and direction carry
  over, except through a gearbox or a reversed gearshift, which reverse them.
* **Mesh**: two cogwheels on the same axis. Same size, edge to edge: same speed, opposite
  direction. A small and a large cog, corner to corner: opposite direction, and the large one
  turns at half the speed.

A gearbox behaves as bevel gears do: two shafts at right angles turn with opposed outward speeds,
which is what the solver's tests check against the geometry. A network **locks** when its sources
or its gear loops ask for speeds that cannot all hold, and it **stops** when the machines use more
stress than the sources carry. A machine uses its impact times its speed, so gearing a machine down
also lowers its stress.

## Develop

The kinetic solver (`src/engine.rs`), the direction rules (`src/direction.rs`), the motor settings
(`src/motor.rs`), the crank and wheel rules (`src/sources.rs`), the rules of the machines that work
on a block (`src/work.rs`) and the display helpers (`src/visual.rs`) are plain Rust with no game
types and have unit tests. `src/lib.rs` connects them to the game.

```sh
cargo horizon-mod check        # validates the package against the pinned SDK
cargo horizon-mod build        # writes target/horizon-mod/create.hmod
cargo test --lib               # unit tests
```

`cargo horizon-mod` needs the SDK it fetches into `.horizon/` and a Rust toolchain for the pinned
build, with the `wasm32-unknown-unknown` target.

Textures are generated, not edited by hand: run
`powershell -NoProfile -ExecutionPolicy Bypass -File tools/generate_textures.ps1` to rewrite the
PNG files under `assets/create/textures/`. The block definitions, block states, models, loot tables
and recipes of the crank, the wheel and the three machines are generated the same way by
`tools/generate_machines.ps1`. Every message has a translation key in
`assets/create/lang/`, and a test checks that English and Spanish define the same keys.

## Limitations

* **Mostly checked by tests and package checks, not in the running game.** Whether every block
  draws and turns, the needles, the clutch and gearshift reacting to redstone, and the exact look of
  the models have not been confirmed for the latest changes.
* Speed on screen is shown as the nearest power of two from 1 to 256 RPM; the exact value is in the
  click report. Blocks look at their network every half second, and a clutch or gearshift at its
  redstone signal every tenth of a second.
* No belts yet. The water wheel is a single block and reads only the four blocks beside it; it does
  not take the direction of the current into account, so use the Wrench on it to pick its direction.
* The drill breaks any breakable block in front of it, containers included, and drops the items.
  The saw fells one log at a time, not a whole tree. Machines do nothing while the network stands
  still, is overstressed or locked.
* The textures are simple 16x16 pixel art drawn by `tools/generate_textures.ps1`; they are
  placeholders in spirit and not hand-painted. Sounds are vanilla's, played at the right moments
  (wrench, motor clicks, clutch and gearshift, a grinding drill); the mod ships no audio files.
* Messages and names are available in English and Spanish (`es_es`).

## License

MIT, see [LICENSE](LICENSE).

# Roadmap

How the mod grows from the current MVP. Each milestone is a set of changes that can be built,
validated with `cargo horizon-mod check` and the unit tests, and then tried in the game.

**How to use this file.** Pick the last milestone to build before the next test (for example
"build up to M3"). Work stops there, everything is pushed, and the test list of that milestone
says what to try. The ◆ marks are the suggested stopping points.

Risk is how likely a milestone is to need a fix after the first run in the game, because it
touches something the unit tests and package checks cannot see (drawing, interaction, timing).

## M0 · MVP — done

Creative Motor, shafts, small and large cogwheels, wrench, mechanical drill, kinetic solver
(speeds, ratios, stress, locks), visible rotation, empty-hand network report.

Not yet tried in the game: the drill, and rotation drawing unless already confirmed.

## Where things stand

M0 to M3 are built and were tried in the game ("works well"). M6 (look and sound) was built next, at
the user's request, ahead of M4 and M5, and its look and the M3 blocks were tried again through
Agent Control (motor, shafts, wrench, cogs, drill, gauges, clutch, gearshift, gearbox); the
sneak-click on the motor was not (Agent Control cannot make the player sneak), and of the sounds
only the wrench's and the depot's were heard. Spanish was tried later and works: the network report
and the block names come out translated. M4 and M5 were built last and tried the
same way and work: the hand crank (after a fix: its block definition had inherited an `axis` from
its template, so it never joined a shaft line), the water wheel, the saw, the harvester and the
plough. The test list under each milestone says what to try.

## M1 · Adjustable motor ◆ — built

Change the Creative Motor's speed and direction without a screen.

- Sneak + empty hand on the motor steps its speed through 1, 2, 4 ... 256 RPM and back to 1.
- The wrench on an end face of the motor (a face on its own axis) reverses its direction.
- The setting lives in the block state, so it survives reloads and reaches every client.

Risk: low. Test: set 8, 32 and 256 RPM and watch shafts and a cog train follow; reverse the
motor; break and replace it and check the default (16 RPM).

## M2 · Gauges — built

Speedometer and Stressometer blocks that show the network without opening the chat.

- They stick to a block and face out of it. They read the block behind them, so they are not part
  of the network and use no stress. A needle shows the speed (log scale) or the share of stress in
  use, and an empty-hand click reports the block behind.
- Drawn through the same block entity path as the turning blocks.

Risk: medium (a second custom drawing). Test: place a gauge on a shaft and on a cog; the needle
moves when the motor speed changes, and the stressometer fills as drills are added and goes full
when the network stops.

## M3 · Transmissions ◆ — built

Ways to turn, split and cut a network.

- **Gearbox**: shaft ends on the four faces across its axis, joining networks at right angles with
  bevel-gear polarity. A shaft straight through it comes out reversed.
- **Clutch** and **Gearshift**: a redstone signal cuts the line (clutch) or reverses its far side
  (gearshift). The signal is read every two ticks.
- The solver gained ports per block face; a property test checks that every link has a reciprocal
  from the other side.

Risk: medium to high. The solver part is fully tested; reading a redstone signal and reacting to it
is not. Test: turn a corner with a gearbox and check the direction after it; feed a line through a
clutch with a lever beside it and watch it stop and start; reverse a line with a gearshift; put two
reversed gearshifts in a row and confirm they cancel.

## M4 · More sources — built and tried in the game

Ways to make power other than the Creative Motor.

- **Hand Crank**: right click with an empty hand gives it four steps of charge (five ticks each) at
  32 RPM, so holding the use button, which repeats the click, keeps it turning. Sneak reads the
  network instead.
- **Water Wheel**: each of the four sides across its axis with flowing (not source) water pushes it
  by the torque of that current about the axle (`offset x flow`), 4 RPM per net push up to 16 RPM;
  currents that push opposite ways cancel. The Wrench on an end reverses it.
- The Hand Crank has a shaft end only on the side it is stuck to (`Kind::Stub` in the solver).
- Tried in the game: a shaft on the stuck-on side of a crank turns at 32 RPM and one on the handle
  side does not; a wheel under a one-way channel turns at 4 RPM, and with two equal sources feeding
  the same cell from opposite ends it stops.
- Both carry 256 stress units per RPM; at rest they are plain joints, so a network passes through
  them. A source's capacity follows its speed, so several sources add up and must agree.

Risk: medium (water flow needs reading fluid states). Test: crank a drill into rock; place a wheel
in a stream and check the speed.

## M5 · More machines ◆ — built and tried in the game

Machines that use the stress, in the style of the drill (they act on blocks, no inventories). They
share the drill's casing, orientation, wrench rules and progress counter (`src/work.rs`).

- **Mechanical Saw** (12 stress units per RPM): fells the log in front of it, one block at a time.
  **Mechanical Harvester** (6): reaps a ripe crop and plants it again. **Mechanical Plough** (6):
  tills dirt, grass or path into farmland when there is room above.
- Each has an impact, a work time and the same orientation and wrench rules as the drill.

Risk: medium. Test: a motor, a gear train and two machines at once, watching overstress stop them.

## M6 · Look and sound ◆ — built (done before M4 and M5, at the user's request)

The mod stops borrowing vanilla textures.

- Own textures for every block and the wrench, drawn from code by `tools/generate_textures.ps1`
  (steel, wood, copper, dials, needle, clutch and gearshift cores, drill parts, the wrench icon).
- Sounds: vanilla sound events at the right moments (wrench, a motor click whose pitch rises with
  the speed, clutch and gearshift, a grinding drill). No audio files of its own yet.
- Spanish translation (`es_es`), and every message is now a translation key with arguments, so a
  test can check that both languages define the same keys.

Not done: hand-painted textures, custom audio, and particles beyond the game's own when a block
breaks.

Risk: low for correctness, high for taste. Test: look at every block and item in the creative tab,
including powered clutch and gearshift and a gauge's dial; switch the game to Spanish and check the
names and the click report; listen to the wrench, the motor clicks and a working drill.

## M7 · Items on machines ◆ — built and tried in the game (Depot, Press, Millstone)

Processing that holds items: a **Millstone** and a **Mechanical Press** on a **Depot**.

- The research is done: see [docs/M7-research.md](docs/M7-research.md). The SDK has what is needed
  (mod block containers that hoppers use, item slots, custom recipe types read as data, items drawn
  on a block entity); five details are still to be confirmed in the game.
- Suggested build order: **M7a** Depot (container, right click in and out, item drawn on top),
  **M7b** Press (a `create:pressing` recipe, progress, a ram), **M7c** Millstone (input and output
  slots, a `create:milling` recipe).

M7a works: right click puts one item on the depot and an empty hand takes it back; hoppers above and
below fill and empty it; the item is drawn on the block and changes show at once (see the research
note for how the client is told). The item lies flat on the depot, and a stack is drawn as a pile of
up to three copies (one item, two to sixteen, more); the exact count is not written.

M7b works: a press above a depot turned 1 iron ingot into 9 nuggets and 1 gold ingot into 9 gold
nuggets, did nothing to an item no recipe takes, and its ram moves only while it works. The recipe
type of the mod's own had to be declared at registration (see the research note). Not done: the
press works one item at a time, so by design a stack of two in a one-slot depot cannot be pressed
(the result has no free slot; not tried), and nothing shows the work in progress besides the ram.

M7c works: right click with an item puts a stack into the millstone, it ground 10 cobblestone into
10 gravel and an empty hand took them out; a hopper above fed it gravel and a hopper below collected
the sand, so a line of hoppers, millstone and hopper runs by itself. The millstone's slots live in
the same block entity type as the other kinetic blocks. Not done: the output is capped at a stack
of 64 whatever the item, only one recipe per input is read (the first that matches), and the stones
spinning inside were drawn but not looked at closely.

## M8 · Logistics (needs research)

**Belts**, **chutes** and **funnels** to move items between machines. A belt is a moving surface
with riders, which is the hardest part of the original mod to reproduce.

Risk: very high. Planned only after M7.

## A speed screen for the motor — built and tried in the game

Built as described below. What was learned in the game: a mod block opens its menu by itself only
when its template is one of a few classes (chest, barrel, hopper...), so the click calls
`player.open_menu(host, entity)`; a menu type needs at least one slot (the motor's is hidden and
unreachable); the screen is drawn with `fill` and `text` and a press arrives as a menu button id.
Tried: the screen opens, a press on the track set -8 RPM (`invert=true`, `rpm=8`) and the `>` button
stepped it to -4. Polish tried later: a tooltip names the speed under the pointer, and the left and
right arrow keys step the speed (256 to 128 to 64 in the game). Not possible with the current SDK:
dragging the knob (the drag hook cannot send a button to the server) and the mouse wheel (the wheel
hook only hears gameplay scrolling, and the menu's type can be read only on the server); a press
anywhere on the track does the job. The labels on the screen are numbers and "RPM", so they need no
translation.

(The original idea, as asked for:)

### Idea · A speed screen for the motor

A small screen opened from the Creative Motor with a slider that sets its speed in both
directions, from -256 to 256 RPM, in place of sneak-clicking through the presets and reversing
with the Wrench.

- The SDK supports it: a block can open a menu of a type the mod declares
  (`data/create/horizon/menu/`), a block entity can keep data slots the menu shows, and the client
  half can lay out and draw the screen and hear clicks, drags and the mouse wheel
  (`screen_click`, `screen_mouse_scrolled_pre`, `screen_mouse_button_released_pre`). There is
  no ready-made slider widget (only buttons and text fields), so the slider is drawn by the mod.
- Sketch: data slots hold the signed RPM; the slider snaps to the preset steps (powers of two) with
  a plain number field for exact values; the pure `motor` module gains the mapping between the
  slider's position and the signed speed, with tests.
- Open questions: how a drag sends its value to the server (buttons that change a data slot are
  the documented way), and whether to keep sneak-click as a shortcut.

Risk: medium (the first custom screen of the mod).

## Out of scope for now

Contraptions (bearings, pistons, gantries), fluids and pipes, trains, schematics. They are large,
and each would be its own roadmap.

## Always, at every milestone

- Official checks pass: `cargo horizon-mod check`, `cargo horizon-mod build` and the registry
  check of the `.hmod`.
- Every new rule has unit tests in the pure modules, not only in the game adapter.
- Written from scratch: other projects are read to understand behaviour, never copied.
- README and this file are updated with what really works and what was not tried.

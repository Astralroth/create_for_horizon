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

## M1 · Adjustable motor ◆

Change the Creative Motor's speed and direction without a screen.

- Sneak + empty hand on the motor steps its speed through 1, 2, 4 ... 256 RPM and back to 1.
- The wrench on an end face of the motor (a face on its own axis) reverses its direction.
- The setting lives in the block state, so it survives reloads and reaches every client.

Risk: low. Test: set 8, 32 and 256 RPM and watch shafts and a cog train follow; reverse the
motor; break and replace it and check the default (16 RPM).

## M2 · Gauges

Speedometer and Stressometer blocks that show the network without opening the chat.

- Speedometer: a needle that points at the block's speed. Stressometer: needle at the stress
  share of capacity. Both join a network as zero-impact blocks and also report on click.
- Drawn through the same block entity path as the turning blocks.

Risk: medium (a second custom drawing). Test: needle moves when the motor speed changes, and
the stressometer fills as drills are added.

## M3 · Transmissions ◆

Ways to turn, split and cut a network.

- **Gearbox**: a block with shaft ends on four sides that links networks at right angles, with
  the reversals a real gearbox gives. This extends the solver beyond straight shafts.
- **Clutch** and **Gearshift**: cut or reverse a network with a redstone signal.
- Solver tests for every new link rule before any block uses it.

Risk: medium to high. The solver part is fully testable; reading a redstone signal and reacting to
it on time is not. Test: turn a corner with a gearbox, then cut and reverse a line with a lever.

## M4 · More sources

Ways to make power other than the Creative Motor.

- **Hand Crank**: a player holds use to spin it, giving a short burst of speed.
- **Water Wheel**: speed from the flowing water around it.
- A source's capacity follows its speed, so several sources add up and must agree.

Risk: medium (water flow needs reading fluid states). Test: crank a drill into rock; place a wheel
in a stream and check the speed.

## M5 · More machines ◆

Machines that use the stress, in the style of the drill (they act on blocks, no inventories).

- **Mechanical Saw**: fells logs in front of it. **Mechanical Harvester**: reaps crops.
- **Mechanical Plough**: tills ground in front of it.
- Each has an impact, a work time and the same orientation and wrench rules as the drill.

Risk: medium. Test: a motor, a gear train and two machines at once, watching overstress stop them.

## M6 · Look and sound ◆

The mod stops borrowing vanilla textures.

- Own textures for every block and item, generated as small pixel-art PNGs.
- Sounds for the motor, the drill and the wrench; particles on drilling.
- Spanish and other translations.

Risk: low for correctness, high for taste. Test: just look.

## M7 · Items on machines (needs research)

Processing that holds items: a **Millstone** and a **Mechanical Press** on a **Depot**.

- Needs inventories, item entities and recipes. The API has what is required, but it is the
  largest unknown so far, so this milestone starts with a research step and a decision.

Risk: high. Not started until M5 is confirmed.

## M8 · Logistics (needs research)

**Belts**, **chutes** and **funnels** to move items between machines. A belt is a moving surface
with riders, which is the hardest part of the original mod to reproduce.

Risk: very high. Planned only after M7.

## Out of scope for now

Contraptions (bearings, pistons, gantries), fluids and pipes, trains, schematics. They are large,
and each would be its own roadmap.

## Always, at every milestone

- Official checks pass: `cargo horizon-mod check`, `cargo horizon-mod build` and the registry
  check of the `.hmod`.
- Every new rule has unit tests in the pure modules, not only in the game adapter.
- Written from scratch: other projects are read to understand behaviour, never copied.
- README and this file are updated with what really works and what was not tried.

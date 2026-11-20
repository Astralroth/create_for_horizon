//! Create for Horizon: rotating machines for Horizon, written from scratch.
//!
//! `engine` solves kinetic networks with no game types. This file is the adapter: it registers a
//! block part for each machine block, turns the blocks around a click into engine nodes,
//! keeps every block's speed in its state, draws the turning blocks and runs the drill.
use mod_api::{
    self, BlockEntityEvent, BlockEntityPart, BlockEntitySubmitEvent, BlockEntityTickEvent,
    BlockEntityTickReply, BlockEntityTicksEvent, BlockFace, BlockKey, BlockPart, BlockUpdateFlags,
    ClientHost, Compound, Hooks, HostBlockStates, InteractionResult, ItemKey, ItemPart,
    OnBlockPlacedByEvent, PlayerLoggedInEvent, Registrar, RegistrationContext, ServerHost,
    SoundSource, Tag, Text, UseItemOnEvent, UseWithoutItemEvent,
};
use std::sync::OnceLock;

mod direction;
mod engine;
mod messages;
mod motor;
mod visual;

use direction::Direction;
use engine::{Axis, Kind, Pos, Status};

/// How often, in game ticks, a block looks at its network again.
const REFRESH_TICKS: i64 = 10;
/// How often, in game ticks, a clutch or gearshift looks at its redstone signal.
const SIGNAL_TICKS: i64 = 2;
/// Stress units the Creative Motor carries for every RPM it turns at (16 RPM carries 16384).
const MOTOR_CAPACITY: f32 = 1024.0;
/// Stress units the drill uses for every RPM it turns at.
const DRILL_IMPACT: f32 = 8.0;
/// Turns of work, in RPM-ticks, the drill needs for each point of a block's hardness.
const DRILL_WORK_PER_HARDNESS: f32 = 600.0;
/// How often, in game ticks, a working drill makes its grinding sound.
const DRILL_SOUND_TICKS: i64 = 20;
/// The saved counter of how far the drill has got through the block in front of it.
const PROGRESS: &str = "Progress";

/// Block and item keys, resolved once at registration.
struct Keys {
    shaft: BlockKey,
    motor: BlockKey,
    small_cogwheel: BlockKey,
    large_cogwheel: BlockKey,
    drill: BlockKey,
    speedometer: BlockKey,
    stressometer: BlockKey,
    gearbox: BlockKey,
    clutch: BlockKey,
    gearshift: BlockKey,
    wrench: ItemKey,
}

static KEYS: OnceLock<Keys> = OnceLock::new();

fn axis_from(value: &str) -> Option<Axis> {
    match value {
        "x" => Some(Axis::X),
        "y" => Some(Axis::Y),
        "z" => Some(Axis::Z),
        _ => None,
    }
}

impl Keys {
    /// What a block of this kind does in a network, turning about `axis`. `motor_rpm` is the
    /// signed speed a Creative Motor is set to, and `powered` whether a clutch or gearshift has
    /// a redstone signal; other blocks ignore them.
    fn kind_of(&self, block: BlockKey, axis: Axis, motor_rpm: f32, powered: bool) -> Option<Kind> {
        if block == self.gearbox {
            Some(Kind::Gearbox { axis })
        } else if block == self.clutch {
            // A powered clutch lets go.
            Some(Kind::Clutch {
                axis,
                engaged: !powered,
            })
        } else if block == self.gearshift {
            // A powered gearshift reverses.
            Some(Kind::Gearshift {
                axis,
                reversed: powered,
            })
        } else if block == self.shaft {
            Some(Kind::Shaft { axis })
        } else if block == self.motor {
            Some(Kind::Generator {
                axis,
                rpm: motor_rpm,
                capacity: MOTOR_CAPACITY,
            })
        } else if block == self.small_cogwheel {
            Some(Kind::Cog { axis, large: false })
        } else if block == self.large_cogwheel {
            Some(Kind::Cog { axis, large: true })
        } else if block == self.drill {
            Some(Kind::Consumer {
                axis,
                impact: DRILL_IMPACT,
            })
        } else {
            None
        }
    }

    /// The block a registry name refers to, among the mod's kinetic blocks.
    fn block_named(&self, name: &str) -> Option<BlockKey> {
        match name {
            "create:shaft" => Some(self.shaft),
            "create:creative_motor" => Some(self.motor),
            "create:cogwheel" => Some(self.small_cogwheel),
            "create:large_cogwheel" => Some(self.large_cogwheel),
            "create:mechanical_drill" => Some(self.drill),
            "create:speedometer" => Some(self.speedometer),
            "create:stressometer" => Some(self.stressometer),
            "create:clutch" => Some(self.clutch),
            "create:gearshift" => Some(self.gearshift),
            _ => None,
        }
    }

    /// Whether a block is one of the two gauges, which read the block they sit on.
    fn is_gauge(&self, block: BlockKey) -> bool {
        block == self.speedometer || block == self.stressometer
    }
}

/// The direction a clicked face points toward.
fn direction_of(face: BlockFace) -> Option<Direction> {
    match face {
        BlockFace::Down => Some(Direction::Down),
        BlockFace::Up => Some(Direction::Up),
        BlockFace::North => Some(Direction::North),
        BlockFace::South => Some(Direction::South),
        BlockFace::West => Some(Direction::West),
        BlockFace::East => Some(Direction::East),
        _ => None,
    }
}

/// The block a gauge at `pos` sits on: behind the face it looks out of.
fn gauge_support(
    host: &dyn mod_api::Host,
    level: &mod_api::LevelRef,
    pos: Pos,
) -> Option<Pos> {
    let facing = Direction::from_name(&level.block_property(host, pos, "facing")?)?;
    Some(facing.opposite().step(pos))
}

/// The axis a kinetic block turns about: its `axis`, or the axis of its `facing`.
fn turning_axis(properties: impl Fn(&str) -> Option<String>) -> Option<Axis> {
    match properties("axis") {
        Some(axis) => axis_from(&axis),
        None => Direction::from_name(&properties("facing")?).map(Direction::axis),
    }
}

/// Reads the block at `pos` as a node of a network.
fn node_at(
    keys: &Keys,
    level: &mod_api::LevelRef,
    host: &dyn mod_api::Host,
    pos: Pos,
) -> Option<Kind> {
    let block = level.block(host, pos)?;
    let axis = turning_axis(|name| level.block_property(host, pos, name))?;
    let motor_rpm = if block == keys.motor {
        let (rpm, invert) = motor::read(|name| level.block_property(host, pos, name));
        motor::signed(rpm, invert)
    } else {
        0.0
    };
    let powered = (block == keys.clutch || block == keys.gearshift)
        && level.block_property(host, pos, "powered").as_deref() == Some("true");
    keys.kind_of(block, axis, motor_rpm, powered)
}

/// The tick offset that spreads the blocks' periodic work out, so they do not all look at once.
fn phase(pos: Pos) -> i64 {
    i64::from(pos[0]) * 31 + i64::from(pos[1]) * 17 + i64::from(pos[2]) * 13
}

/// Copies the redstone signal a clutch or gearshift gets into its `powered` state, which the
/// network solver reads and the client draws.
fn refresh_signal(host: &mut dyn ServerHost, pos: Pos) -> Option<()> {
    let keys = KEYS.get()?;
    let level = host.level(&host.dimension())?;
    if (level.game_time(host)? + phase(pos)).rem_euclid(SIGNAL_TICKS) != 0 {
        return Some(());
    }
    let block = level.block(host, pos)?;
    if block != keys.clutch && block != keys.gearshift {
        return Some(());
    }
    let value = if level.has_neighbor_signal(host, pos) {
        "true"
    } else {
        "false"
    };
    if level.block_property(host, pos, "powered").as_deref() != Some(value) {
        host.set_block_property_with(pos, "powered", value, BlockUpdateFlags::CLIENTS.bits());
        // A click, higher when it lets go or reverses and lower when it lets up.
        let pitch = if value == "true" { 0.8 } else { 0.7 };
        play(host, &level, pos, "minecraft:block.lever.click", 0.5, pitch);
    }
    Some(())
}

/// Looks at the network the block at `pos` belongs to and, when it is time, writes how fast the
/// block turns into its state so that every client draws it.
fn refresh(host: &mut dyn ServerHost, pos: Pos) -> Option<()> {
    let keys = KEYS.get()?;
    let level = host.level(&host.dimension())?;
    // Spread the work: each block looks every REFRESH_TICKS, at an offset of its own.
    if (level.game_time(host)? + phase(pos)).rem_euclid(REFRESH_TICKS) != 0 {
        return Some(());
    }
    let block = level.block(host, pos)?;
    if keys.is_gauge(block) {
        return refresh_gauge(host, &level, keys, pos, block == keys.speedometer);
    }
    let network = {
        let reader: &dyn mod_api::Host = host;
        engine::solve(|at| node_at(keys, &level, reader, at), pos)?
    };
    let rpm = network.member(pos)?.rpm;
    let shown = visual::level(rpm);
    let speed = visual::level_name(shown);
    let reverse = if shown > 0 && rpm < 0.0 { "true" } else { "false" };
    for (key, value) in [("speed", speed), ("reverse", reverse)] {
        if level.block_property(host, pos, key).as_deref() != Some(value) {
            // Tell clients, but not the neighbours: nothing here reacts to a neighbour change.
            host.set_block_property_with(pos, key, value, BlockUpdateFlags::CLIENTS.bits());
        }
    }
    Some(())
}

/// Writes the dial step a gauge shows: the speed or the stress of the block it sits on.
fn refresh_gauge(
    host: &mut dyn ServerHost,
    level: &mod_api::LevelRef,
    keys: &Keys,
    pos: Pos,
    speedometer: bool,
) -> Option<()> {
    let dial = {
        let reader: &dyn mod_api::Host = host;
        let support = gauge_support(reader, level, pos)?;
        match engine::solve(|at| node_at(keys, level, reader, at), support) {
            None => 0,
            Some(network) if speedometer => {
                let rpm = network.member(support).map_or(0.0, |member| member.rpm);
                visual::speed_dial(visual::level(rpm))
            }
            Some(network) => visual::stress_dial(
                network.stress,
                network.capacity,
                network.status == Status::Overstressed,
            ),
        }
    };
    let value = visual::dial_name(dial);
    if level.block_property(host, pos, "dial").as_deref() != Some(value.as_str()) {
        host.set_block_property_with(pos, "dial", &value, BlockUpdateFlags::CLIENTS.bits());
    }
    Some(())
}

/// How many RPM-ticks of work the drill at `pos` has done on the block in front of it, and the
/// work that block needs, or `None` when it cannot be drilled.
fn drill_job(
    host: &dyn mod_api::Host,
    level: &mod_api::LevelRef,
    pos: Pos,
) -> Option<(Pos, f32)> {
    let facing = Direction::from_name(&level.block_property(host, pos, "facing")?)?;
    let target = facing.step(pos);
    let states = host.block_states();
    let state = level.block_state(host, target)?;
    let hardness = states.destroy_speed(state)?;
    // A negative hardness cannot be broken (bedrock); replaceable blocks are air, fluids and
    // plants, which there is nothing to drill.
    if hardness < 0.0 || states.replaceable(state)? {
        return None;
    }
    Some((target, hardness.max(0.5) * DRILL_WORK_PER_HARDNESS))
}

/// One tick of a drill: it works while it turns, and breaks the block in front once it has done
/// the work that block needs.
fn drill(host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) -> Option<()> {
    let done = match event.entity.nbt.get(&PROGRESS.into()) {
        Some(Tag::Int(value)) => *value,
        // Not a drill: only a drill has the counter.
        _ => return Some(()),
    };
    let pos = event.pos;
    let level = host.level(&host.dimension())?;
    let turning = visual::level_from_name(&level.block_property(host, pos, "speed")?)?;
    let job = if turning > 0 {
        let reader: &dyn mod_api::Host = host;
        drill_job(reader, &level, pos)
    } else {
        None
    };
    if job.is_some() && (level.game_time(host)? + phase(pos)).rem_euclid(DRILL_SOUND_TICKS) == 0 {
        // A low grinding while it works, once a second.
        play(host, &level, pos, "minecraft:block.grindstone.use", 0.25, 0.9);
    }
    let next = match job {
        Some((target, needed)) => {
            let total = done as f32 + turning as f32;
            if total >= needed {
                let _ = level.destroy_block(host, target, true, None, false);
                0
            } else {
                total as i32
            }
        }
        // Idle, stopped or nothing to drill: the progress is lost.
        None => 0,
    };
    if next != done {
        event.entity.nbt.insert(PROGRESS.into(), Tag::Int(next));
        *event.reply = BlockEntityTickReply::Changed;
    }
    Some(())
}

/// Game text for a message: its translation key with the arguments filled in.
fn text(message: messages::Message) -> Text {
    Text::translate(
        message.key,
        message.arguments.into_iter().map(Text::literal).collect(),
    )
}

/// Plays a vanilla sound at the centre of the block at `pos`.
fn play(
    host: &mut dyn ServerHost,
    level: &mod_api::LevelRef,
    pos: Pos,
    sound: &str,
    volume: f32,
    pitch: f32,
) {
    let at = pos.map(|coordinate| f64::from(coordinate) + 0.5);
    let _ = level.play_sound(host, sound, at, SoundSource::Blocks, volume, pitch);
}

/// Whether a click on a machine block shows the network report: only with an empty hand.
/// Any held item keeps the click, so blocks can be placed against machines without sneaking.
fn reports_on_click(holding_item: bool) -> bool {
    !holding_item
}

/// Behaviour shared by every machine block.
struct Machine;

#[mod_api::horizon_part]
impl BlockPart for Machine {
    /// Right click with the wrench: turn the block a quarter turn about the face that was
    /// clicked, so connected blocks can be lined up without breaking them.
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let keys = KEYS.get()?;
        if click.stack?.item() != keys.wrench {
            return None;
        }
        let player = host.player(click.player)?;
        let level = player.level(host)?;
        let face = Axis::of_face(click.hit.face)?;
        if let Some(current) = level.block_property(host, click.pos, "axis") {
            let current = axis_from(&current)?;
            if level.block(host, click.pos)? == keys.motor && face == current {
                // The wrench on a shaft end of the motor reverses it instead of turning it.
                let inverted =
                    level.block_property(host, click.pos, "invert").as_deref() == Some("true");
                host.set_block_property(
                    click.pos,
                    "invert",
                    if inverted { "false" } else { "true" },
                );
                let message = messages::motor_direction(!inverted);
                let _ = player.tell(host, text(message));
                play(host, &level, click.pos, "minecraft:block.chain.place", 0.7, 1.3);
                return Some(InteractionResult::SuccessServer);
            }
            host.set_block_property(click.pos, "axis", current.turned_about(face).name());
        } else {
            let current = Direction::from_name(&level.block_property(host, click.pos, "facing")?)?;
            host.set_block_property(click.pos, "facing", current.turned_about(face).name());
        }
        play(host, &level, click.pos, "minecraft:block.chain.place", 0.7, 1.0);
        Some(InteractionResult::SuccessServer)
    }

    /// Right click with an empty hand: report the kinetic network the block belongs to.
    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        // The game tries this after the held item's own use, even when the hand is not empty.
        // With an item in hand the click belongs to the item, for example a block being placed
        // against this one, so answer nothing and let it go on.
        if !reports_on_click(click.stack.is_some()) {
            return None;
        }
        let keys = KEYS.get()?;
        let player = host.player(click.player)?;
        let level = player.level(host)?;
        if level.block(host, click.pos)? == keys.motor
            && player.is_secondary_use_active(host) == Some(true)
        {
            // Sneaking on the motor steps its speed through the presets.
            let (rpm, invert) = motor::read(|name| level.block_property(host, click.pos, name));
            let next = motor::next(rpm);
            host.set_block_property(click.pos, "rpm", motor::name(next));
            let _ = player.tell(host, text(messages::motor_speed(next, invert)));
            play(
                host,
                &level,
                click.pos,
                "minecraft:block.lever.click",
                0.8,
                motor::click_pitch(next),
            );
            return Some(InteractionResult::SuccessServer);
        }
        // A gauge reports the block it sits on; anything else reports itself.
        let block = level.block(host, click.pos)?;
        let start = if keys.is_gauge(block) {
            gauge_support(host, &level, click.pos)?
        } else {
            click.pos
        };
        let network = {
            let reader: &dyn mod_api::Host = host;
            engine::solve(|pos| node_at(keys, &level, reader, pos), start)
        };
        let message = match network {
            Some(network) => messages::report(&network, start),
            None => messages::Message::plain(messages::GAUGE_EMPTY),
        };
        let _ = player.tell(host, text(message));
        Some(InteractionResult::SuccessServer)
    }
}

/// Makes a drill face where its player looks when they place it.
struct FacesWherePlacerLooks;

#[mod_api::horizon_part]
impl ItemPart for FacesWherePlacerLooks {
    fn on_block_placed_by(&self, host: &mut dyn ServerHost, event: &mut OnBlockPlacedByEvent<'_>) {
        let Some(placer) = event.placer else { return };
        let Some([yaw, pitch]) = placer.entity().rotation(host) else {
            return;
        };
        let facing = Direction::from_look(yaw, pitch);
        host.set_block_property(event.pos, "facing", facing.name());
    }
}

/// Makes a gauge face away from the block it was placed against.
struct FacesAwayFromSupport;

#[mod_api::horizon_part]
impl ItemPart for FacesAwayFromSupport {
    fn on_block_placed_by(&self, host: &mut dyn ServerHost, event: &mut OnBlockPlacedByEvent<'_>) {
        if let Some(facing) = direction_of(event.face) {
            host.set_block_property(event.pos, "facing", facing.name());
        }
    }
}

/// The block entity every kinetic block has: it looks at its network on the server and draws
/// the turning block on the client.
struct Kinetic;

/// A block entity part also needs its block behaviour; `Machine` has all of it already.
#[mod_api::horizon_part]
impl BlockPart for Kinetic {}

#[mod_api::horizon_part]
impl BlockEntityPart for Kinetic {
    /// A drill keeps a counter of its progress; every other block has no saved data.
    fn block_entity(
        &self,
        _: mod_api::LevelSide,
        event: &mut BlockEntityEvent<'_>,
    ) -> Option<Compound> {
        (event.block == "create:mechanical_drill").then(|| {
            let mut data = Compound::new();
            data.insert(PROGRESS.into(), Tag::Int(0));
            data
        })
    }

    fn block_entity_ticks(&self, _: &dyn mod_api::Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        // Only the drill edits saved data, and then it says so, so the chunk is not marked
        // dirty by a block that is only turning.
        *event.reply = BlockEntityTickReply::Unchanged;
        let _ = refresh_signal(host, event.pos);
        let _ = refresh(host, event.pos);
        let _ = drill(host, event);
    }

    fn block_entity_submit(&self, host: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let Some(keys) = KEYS.get() else { return };
        let Some(block) = keys.block_named(event.block) else {
            return;
        };
        let states = host.block_states();
        let property = |name: &str| event.state.property(&states, name).ok().flatten();
        let level = property("speed")
            .and_then(visual::level_from_name)
            .unwrap_or(0);
        let reverse = property("reverse") == Some("true");
        let degrees = visual::degrees(level, reverse, event.draw.animation_time());
        let dial = property("dial")
            .and_then(visual::dial_from_name)
            .unwrap_or(0);
        let draw = &mut *event.draw;
        if keys.is_gauge(block) {
            // The plate stays still and the needle turns about the axis the gauge faces along.
            let Some(facing) = property("facing") else {
                return;
            };
            let Some(direction) = Direction::from_name(facing) else {
                return;
            };
            let positive = matches!(direction, Direction::Up | Direction::East | Direction::South);
            draw.push();
            draw.block(block, &[("facing", facing), ("part", "body")]);
            draw.pop();
            draw.push();
            draw.rotate_about(
                axis_vector(direction.axis()),
                visual::needle_degrees(dial, positive),
                [0.5; 3],
            );
            draw.block(block, &[("facing", facing), ("part", "needle")]);
            draw.pop();
            return;
        }
        if block == keys.drill {
            // The body stays still and the head turns about the drill's axis.
            let Some(facing) = property("facing") else {
                return;
            };
            let Some(direction) = Direction::from_name(facing) else {
                return;
            };
            draw.push();
            draw.block(block, &[("facing", facing), ("part", "body")]);
            draw.pop();
            draw.push();
            draw.rotate_about(axis_vector(direction.axis()), degrees, [0.5; 3]);
            draw.block(block, &[("facing", facing), ("part", "head")]);
            draw.pop();
            return;
        }
        let Some(axis_name) = property("axis") else {
            return;
        };
        let Some(axis) = axis_from(axis_name) else {
            return;
        };
        // A clutch or gearshift also has a model for when it is powered.
        let mut properties = vec![("axis", axis_name)];
        if let Some(powered) = property("powered") {
            properties.push(("powered", powered));
        }
        draw.push();
        draw.rotate_about(axis_vector(axis), degrees, [0.5; 3]);
        draw.block(block, &properties);
        draw.pop();
    }
}

fn axis_vector(axis: Axis) -> [f32; 3] {
    match axis {
        Axis::X => [1.0, 0.0, 0.0],
        Axis::Y => [0.0, 1.0, 0.0],
        Axis::Z => [0.0, 0.0, 1.0],
    }
}

#[derive(Default)]
struct CreateMod;

#[mod_api::horizon_mod]
impl Hooks for CreateMod {
    /// Attach the machine behaviour to the mod's blocks.
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        let keys = Keys {
            shaft: registrar.block_key("create:shaft"),
            motor: registrar.block_key("create:creative_motor"),
            small_cogwheel: registrar.block_key("create:cogwheel"),
            large_cogwheel: registrar.block_key("create:large_cogwheel"),
            drill: registrar.block_key("create:mechanical_drill"),
            speedometer: registrar.block_key("create:speedometer"),
            stressometer: registrar.block_key("create:stressometer"),
            gearbox: registrar.block_key("create:gearbox"),
            clutch: registrar.block_key("create:clutch"),
            gearshift: registrar.block_key("create:gearshift"),
            wrench: registrar.item_key("create:wrench"),
        };
        for block in [
            keys.shaft,
            keys.motor,
            keys.small_cogwheel,
            keys.large_cogwheel,
            keys.drill,
            keys.speedometer,
            keys.stressometer,
            keys.gearbox,
            keys.clutch,
            keys.gearshift,
        ] {
            registrar.block_part(block, Machine);
        }
        let kinetic = registrar
            .block_entity_type("kinetic")
            .blocks(&[
                "shaft",
                "creative_motor",
                "cogwheel",
                "large_cogwheel",
                "mechanical_drill",
                "speedometer",
                "stressometer",
                "clutch",
                "gearshift",
            ])
            .build();
        registrar
            .block_entity_part(&kinetic, Kinetic)
            .expect("kinetic is a native block entity type");
        let drill_item = registrar.item_key("create:mechanical_drill");
        registrar.item_part(drill_item, FacesWherePlacerLooks);
        for name in ["create:speedometer", "create:stressometer"] {
            let gauge_item = registrar.item_key(name);
            registrar.item_part(gauge_item, FacesAwayFromSupport);
        }
        let _ = KEYS.set(keys);
    }

    /// A player joined: say that the mod is installed.
    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            let _ = player.tell(host, text(messages::Message::plain(messages::LOADED)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_item_keeps_the_click() {
        // Regression: the report used to consume every click, so a block could not be placed
        // against a shaft without sneaking.
        assert!(reports_on_click(false));
        assert!(!reports_on_click(true));
    }

    #[test]
    fn axes_parse_from_state_values() {
        assert_eq!(axis_from("x"), Some(Axis::X));
        assert_eq!(axis_from("y"), Some(Axis::Y));
        assert_eq!(axis_from("z"), Some(Axis::Z));
        assert_eq!(axis_from("north"), None);
    }

    #[test]
    fn the_turning_axis_comes_from_axis_or_facing() {
        let axis_block = |name: &str| (name == "axis").then(|| "z".to_string());
        assert_eq!(turning_axis(axis_block), Some(Axis::Z));
        let facing_block = |name: &str| (name == "facing").then(|| "east".to_string());
        assert_eq!(turning_axis(facing_block), Some(Axis::X));
        let down = |name: &str| (name == "facing").then(|| "down".to_string());
        assert_eq!(turning_axis(down), Some(Axis::Y));
        assert_eq!(turning_axis(|_| None), None);
        // A facing nobody knows is not an axis either.
        assert_eq!(turning_axis(|name| (name == "facing").then(|| "sideways".to_string())), None);
    }
}

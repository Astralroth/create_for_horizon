//! Create for Horizon: rotating machines for Horizon, written from scratch.
//!
//! `engine` solves kinetic networks with no game types. This file is the adapter: it registers a
//! block part for each machine block, turns the blocks around a click into engine nodes,
//! keeps every block's speed in its state, draws the turning blocks and runs the drill.
use mod_api::{
    self, BlockEntityEvent, BlockEntityPart, BlockEntitySubmitEvent, BlockEntityTickEvent,
    BlockEntityTickReply, BlockEntityTicksEvent, BlockKey, BlockPart, BlockUpdateFlags,
    ClientHost, Compound, Hooks, HostBlockStates, InteractionResult, ItemKey, ItemPart,
    OnBlockPlacedByEvent, PlayerLoggedInEvent, Registrar, RegistrationContext, ServerHost, Tag,
    Text, UseItemOnEvent, UseWithoutItemEvent,
};
use std::sync::OnceLock;

mod direction;
mod engine;
mod visual;

use direction::Direction;
use engine::{Axis, Kind, Network, Pos, Status};

/// How often, in game ticks, a block looks at its network again.
const REFRESH_TICKS: i64 = 10;
/// The speed of the Creative Motor until it becomes settable.
const MOTOR_RPM: f32 = 16.0;
/// Stress units the Creative Motor carries for every RPM it turns at (16 RPM carries 16384).
const MOTOR_CAPACITY: f32 = 1024.0;
/// Stress units the drill uses for every RPM it turns at.
const DRILL_IMPACT: f32 = 8.0;
/// Turns of work, in RPM-ticks, the drill needs for each point of a block's hardness.
const DRILL_WORK_PER_HARDNESS: f32 = 600.0;
/// The saved counter of how far the drill has got through the block in front of it.
const PROGRESS: &str = "Progress";

/// Block and item keys, resolved once at registration.
struct Keys {
    shaft: BlockKey,
    motor: BlockKey,
    small_cogwheel: BlockKey,
    large_cogwheel: BlockKey,
    drill: BlockKey,
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
    /// What a block of this kind does in a network, turning about `axis`.
    fn kind_of(&self, block: BlockKey, axis: Axis) -> Option<Kind> {
        if block == self.shaft {
            Some(Kind::Shaft { axis })
        } else if block == self.motor {
            Some(Kind::Generator {
                axis,
                rpm: MOTOR_RPM,
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
            _ => None,
        }
    }
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
    keys.kind_of(block, axis)
}

/// Looks at the network the block at `pos` belongs to and, when it is time, writes how fast the
/// block turns into its state so that every client draws it.
fn refresh(host: &mut dyn ServerHost, pos: Pos) -> Option<()> {
    let keys = KEYS.get()?;
    let level = host.level(&host.dimension())?;
    // Spread the work: each block looks every REFRESH_TICKS, at an offset of its own.
    let phase = i64::from(pos[0]) * 31 + i64::from(pos[1]) * 17 + i64::from(pos[2]) * 13;
    if (level.game_time(host)? + phase).rem_euclid(REFRESH_TICKS) != 0 {
        return Some(());
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

/// The message a click on the block at `pos` shows.
fn describe(network: &Network, pos: Pos) -> String {
    let blocks = network.members.len();
    let rpm = network.member(pos).map_or(0.0, |member| member.rpm);
    match network.status {
        Status::Idle => format!("Create: {blocks} connected, no source, nothing turns."),
        Status::Running => format!(
            "Create: turning at {} RPM. Network of {blocks}, stress {:.0} of {:.0}.",
            rpm.abs(),
            network.stress,
            network.capacity
        ),
        Status::Conflict => {
            format!("Create: {blocks} connected, but the sources or gears disagree. Locked.")
        }
        Status::Overstressed => format!(
            "Create: overstressed, {:.0} of {:.0} stress units. Nothing turns.",
            network.stress, network.capacity
        ),
    }
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
            let turned = axis_from(&current)?.turned_about(face);
            host.set_block_property(click.pos, "axis", turned.name());
        } else {
            let current = Direction::from_name(&level.block_property(host, click.pos, "facing")?)?;
            host.set_block_property(click.pos, "facing", current.turned_about(face).name());
        }
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
        let network = {
            let reader: &dyn mod_api::Host = host;
            engine::solve(|pos| node_at(keys, &level, reader, pos), click.pos)?
        };
        let _ = player.tell(host, Text::literal(describe(&network, click.pos)));
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
        let draw = &mut *event.draw;
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
        draw.push();
        draw.rotate_about(axis_vector(axis), degrees, [0.5; 3]);
        draw.block(block, &[("axis", axis_name)]);
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
            wrench: registrar.item_key("create:wrench"),
        };
        for block in [
            keys.shaft,
            keys.motor,
            keys.small_cogwheel,
            keys.large_cogwheel,
            keys.drill,
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
            ])
            .build();
        registrar
            .block_entity_part(&kinetic, Kinetic)
            .expect("kinetic is a native block entity type");
        let drill_item = registrar.item_key("create:mechanical_drill");
        registrar.item_part(drill_item, FacesWherePlacerLooks);
        let _ = KEYS.set(keys);
    }

    /// A player joined: say that the mod is installed.
    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            let _ = player.tell(host, Text::literal("Create for Horizon is loaded."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{Member, solve};

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

    fn line(kinds: &[Kind]) -> Network {
        let nodes: Vec<(Pos, Kind)> = kinds
            .iter()
            .enumerate()
            .map(|(x, kind)| ([x as i32, 0, 0], *kind))
            .collect();
        solve(
            |pos| nodes.iter().find(|(at, _)| *at == pos).map(|(_, kind)| *kind),
            [0, 0, 0],
        )
        .unwrap()
    }

    fn motor(rpm: f32) -> Kind {
        Kind::Generator {
            axis: Axis::X,
            rpm,
            capacity: MOTOR_CAPACITY,
        }
    }

    #[test]
    fn descriptions_name_the_state_of_the_network() {
        let idle = line(&[Kind::Shaft { axis: Axis::X }]);
        assert!(describe(&idle, [0, 0, 0]).contains("nothing turns"));

        let running = line(&[motor(16.0), Kind::Shaft { axis: Axis::X }]);
        let text = describe(&running, [1, 0, 0]);
        assert!(text.contains("16 RPM"), "{text}");
        assert!(text.contains("Network of 2"), "{text}");

        let locked = line(&[motor(16.0), Kind::Shaft { axis: Axis::X }, motor(-16.0)]);
        assert!(describe(&locked, [0, 0, 0]).contains("Locked"));

        let overstressed = line(&[
            motor(1.0),
            Kind::Consumer {
                axis: Axis::X,
                impact: 5000.0,
            },
        ]);
        assert!(describe(&overstressed, [0, 0, 0]).contains("overstressed"));
    }

    #[test]
    fn a_drill_on_a_motor_shows_its_stress() {
        let network = line(&[
            motor(16.0),
            Kind::Consumer {
                axis: Axis::X,
                impact: DRILL_IMPACT,
            },
        ]);
        // 16 RPM x 8 stress units per RPM = 128 of the motor's 16384.
        assert!(describe(&network, [1, 0, 0]).contains("stress 128 of 16384"));
    }

    #[test]
    fn the_speed_shown_is_the_one_of_the_clicked_block() {
        let network = Network {
            members: vec![
                Member {
                    pos: [0, 0, 0],
                    kind: Kind::Shaft { axis: Axis::X },
                    rpm: -8.0,
                },
                Member {
                    pos: [1, 0, 0],
                    kind: Kind::Shaft { axis: Axis::X },
                    rpm: 8.0,
                },
            ],
            status: Status::Running,
            stress: 0.0,
            capacity: 100.0,
        };
        assert!(describe(&network, [0, 0, 0]).contains("8 RPM"));
        // Direction is not part of the message, only the speed.
        assert!(!describe(&network, [0, 0, 0]).contains('-'));
    }
}

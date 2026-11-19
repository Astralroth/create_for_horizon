//! Create for Horizon: rotating machines for Horizon, written from scratch.
//!
//! `engine` solves kinetic networks with no game types. This file is the adapter: it registers a
//! block part for each machine block, turns the blocks around a click into engine nodes and
//! reports what the solver says.
use mod_api::{
    self, BlockEntityPart, BlockEntitySubmitEvent, BlockEntityTickEvent, BlockEntityTickReply,
    BlockEntityTicksEvent, BlockKey, BlockPart, BlockUpdateFlags, ClientHost, Hooks,
    HostBlockStates, InteractionResult, ItemKey, PlayerLoggedInEvent, Registrar,
    RegistrationContext, ServerHost, Text, UseItemOnEvent, UseWithoutItemEvent,
};
use std::sync::OnceLock;

mod engine;
mod visual;

use engine::{Axis, Kind, Network, Pos, Status};

/// How often, in game ticks, a block looks at its network again.
const REFRESH_TICKS: i64 = 10;

/// The speed of the Creative Motor until it becomes settable.
const MOTOR_RPM: f32 = 16.0;
/// Stress units the Creative Motor carries for every RPM it turns at (16 RPM carries 16384).
const MOTOR_CAPACITY: f32 = 1024.0;

/// Block keys, resolved once at registration.
struct Keys {
    shaft: BlockKey,
    motor: BlockKey,
    small_cogwheel: BlockKey,
    large_cogwheel: BlockKey,
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
        } else {
            None
        }
    }
}

impl Keys {
    /// The block a registry name refers to, among the mod's kinetic blocks.
    fn block_named(&self, name: &str) -> Option<BlockKey> {
        match name {
            "create:shaft" => Some(self.shaft),
            "create:creative_motor" => Some(self.motor),
            "create:cogwheel" => Some(self.small_cogwheel),
            "create:large_cogwheel" => Some(self.large_cogwheel),
            _ => None,
        }
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
    let axis = axis_from(&level.block_property(host, pos, "axis")?)?;
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
        let current = axis_from(&level.block_property(host, click.pos, "axis")?)?;
        let face = Axis::of_face(click.hit.face)?;
        let turned = current.turned_about(face);
        host.set_block_property(click.pos, "axis", turned.name());
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
            engine::solve(
                |pos| {
                    let block = level.block(reader, pos)?;
                    let axis = axis_from(&level.block_property(reader, pos, "axis")?)?;
                    keys.kind_of(block, axis)
                },
                click.pos,
            )?
        };
        let _ = player.tell(host, Text::literal(describe(&network, click.pos)));
        Some(InteractionResult::SuccessServer)
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
    fn block_entity_ticks(&self, _: &dyn mod_api::Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        // The tick edits no saved data, so the chunk is not marked dirty.
        *event.reply = BlockEntityTickReply::Unchanged;
        let _ = refresh(host, event.pos);
    }

    fn block_entity_submit(&self, host: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let Some(keys) = KEYS.get() else { return };
        let Some(block) = keys.block_named(event.block) else {
            return;
        };
        let states = host.block_states();
        let property = |name: &str| event.state.property(&states, name).ok().flatten();
        let Some(axis_name) = property("axis") else {
            return;
        };
        let level = property("speed")
            .and_then(visual::level_from_name)
            .unwrap_or(0);
        let reverse = property("reverse") == Some("true");
        let vector = match axis_from(axis_name) {
            Some(Axis::X) => [1.0, 0.0, 0.0],
            Some(Axis::Y) => [0.0, 1.0, 0.0],
            Some(Axis::Z) => [0.0, 0.0, 1.0],
            None => return,
        };
        let degrees = visual::degrees(level, reverse, event.draw.animation_time());
        let draw = &mut *event.draw;
        draw.push();
        draw.rotate_about(vector, degrees, [0.5; 3]);
        draw.block(block, &[("axis", axis_name)]);
        draw.pop();
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
            wrench: registrar.item_key("create:wrench"),
        };
        for block in [
            keys.shaft,
            keys.motor,
            keys.small_cogwheel,
            keys.large_cogwheel,
        ] {
            registrar.block_part(block, Machine);
        }
        let kinetic = registrar
            .block_entity_type("kinetic")
            .blocks(&["shaft", "creative_motor", "cogwheel", "large_cogwheel"])
            .build();
        registrar
            .block_entity_part(&kinetic, Kinetic)
            .expect("kinetic is a native block entity type");
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

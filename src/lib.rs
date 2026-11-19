//! Create for Horizon: rotating machines for Horizon, written from scratch.
//!
//! `engine` solves kinetic networks with no game types. This file is the adapter: it registers a
//! block part for each machine block, turns the blocks around a click into engine nodes and
//! reports what the solver says.
use mod_api::{
    self, BlockKey, BlockPart, Hooks, InteractionResult, PlayerLoggedInEvent, Registrar,
    RegistrationContext, ServerHost, Text, UseWithoutItemEvent,
};
use std::sync::OnceLock;

mod engine;

use engine::{Axis, Kind, Network, Pos, Status};

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

/// Behaviour shared by every machine block.
struct Machine;

#[mod_api::horizon_part]
impl BlockPart for Machine {
    /// Right click with an empty hand: report the kinetic network the block belongs to.
    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let keys = KEYS.get()?;
        let click = event.click;
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
        };
        for block in [
            keys.shaft,
            keys.motor,
            keys.small_cogwheel,
            keys.large_cogwheel,
        ] {
            registrar.block_part(block, Machine);
        }
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

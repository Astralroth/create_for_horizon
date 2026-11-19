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

// Parts of the solver the adapter does not need yet stay unused for now.
#[allow(dead_code)]
mod engine;

use engine::{Axis, Kind, Status};

/// The speed of the Creative Motor until it becomes settable.
const MOTOR_RPM: f32 = 16.0;

/// Block keys, resolved once at registration.
struct Keys {
    shaft: BlockKey,
    motor: BlockKey,
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

/// The message a click on a machine block shows.
fn describe(network: &engine::Network) -> String {
    let blocks = network.members.len();
    match network.status {
        Status::Idle => format!("Create: {blocks} connected, no source, nothing turns."),
        Status::Running => format!(
            "Create: {blocks} connected, {} source(s), turning at {} RPM.",
            network.sources.len(),
            network.rpm
        ),
        Status::Conflict => format!(
            "Create: {blocks} connected, {} sources disagree, the network is locked.",
            network.sources.len()
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
                    if block == keys.shaft {
                        Some(Kind::Shaft { axis })
                    } else if block == keys.motor {
                        Some(Kind::Generator {
                            axis,
                            rpm: MOTOR_RPM,
                        })
                    } else {
                        None
                    }
                },
                click.pos,
            )?
        };
        let _ = player.tell(host, Text::literal(describe(&network)));
        Some(InteractionResult::SuccessServer)
    }
}

#[derive(Default)]
struct CreateMod;

#[mod_api::horizon_mod]
impl Hooks for CreateMod {
    /// Attach the machine behaviour to the mod's blocks.
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        let shaft = registrar.block_key("create:shaft");
        let motor = registrar.block_key("create:creative_motor");
        let _ = KEYS.set(Keys { shaft, motor });
        registrar.block_part(shaft, Machine);
        registrar.block_part(motor, Machine);
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

    #[test]
    fn axes_parse_from_state_values() {
        assert_eq!(axis_from("x"), Some(Axis::X));
        assert_eq!(axis_from("y"), Some(Axis::Y));
        assert_eq!(axis_from("z"), Some(Axis::Z));
        assert_eq!(axis_from("north"), None);
    }

    #[test]
    fn descriptions_name_the_state_of_the_network() {
        let idle = engine::Network {
            members: vec![[0, 0, 0]],
            sources: vec![],
            status: Status::Idle,
            rpm: 0.0,
        };
        assert!(describe(&idle).contains("nothing turns"));
        let running = engine::Network {
            members: vec![[0, 0, 0], [1, 0, 0]],
            sources: vec![[0, 0, 0]],
            status: Status::Running,
            rpm: 16.0,
        };
        assert!(describe(&running).contains("16 RPM"));
        let locked = engine::Network {
            members: vec![[0, 0, 0]],
            sources: vec![[0, 0, 0], [2, 0, 0]],
            status: Status::Conflict,
            rpm: 0.0,
        };
        assert!(describe(&locked).contains("locked"));
    }
}

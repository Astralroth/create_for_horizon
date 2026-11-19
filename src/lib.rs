//! Create for Horizon: rotating machines for Horizon, written from scratch.
//!
//! This is the package skeleton. It only proves that the package format, the
//! pinned build and the launcher accept the mod; the kinetic engine, blocks and
//! game mixins are added on top of it.
use mod_api::{self, Hooks, PlayerLoggedInEvent, ServerHost, Text};

#[derive(Default)]
struct CreateMod;

#[mod_api::horizon_mod]
impl Hooks for CreateMod {
    /// A player joined: say that the skeleton is installed.
    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            let _ = player.tell(host, Text::literal("Create for Horizon (skeleton) is loaded."));
        }
    }
}

//! The Creative Motor's settings: pure logic with no game types.
//!
//! A motor has two settings kept in its block state: `rpm`, one of a few presets, and `invert`,
//! whether it turns the other way. The signed speed the solver gets is the two together.

use crate::visual;

/// The speeds a motor can be set to, in RPM.
pub const PRESETS: [u32; 9] = [1, 2, 4, 8, 16, 32, 64, 128, 256];

/// The speed of a motor nobody has set.
pub const DEFAULT_RPM: u32 = 16;

/// The preset a state value spells, or `None` for anything else.
pub fn from_name(name: &str) -> Option<u32> {
    PRESETS
        .into_iter()
        .find(|preset| visual::level_name(*preset) == name)
}

/// The state value that spells `rpm`.
pub fn name(rpm: u32) -> &'static str {
    visual::level_name(rpm)
}

/// The preset after `rpm`, wrapping from the fastest back to the slowest. A speed that is not a
/// preset steps to the default.
pub fn next(rpm: u32) -> u32 {
    match PRESETS.iter().position(|preset| *preset == rpm) {
        Some(index) => PRESETS[(index + 1) % PRESETS.len()],
        None => DEFAULT_RPM,
    }
}

/// The pitch of the click heard when the motor is set to `rpm`: low for the slowest preset and
/// rising with each step, so the sound tells how fast the motor now is.
pub fn click_pitch(rpm: u32) -> f32 {
    let index = PRESETS.iter().position(|preset| *preset == rpm).unwrap_or(4);
    0.5 + 0.15 * index as f32
}

/// The signed speed of a motor set to `rpm`: positive turns counterclockwise seen from the
/// positive end of its axis, and `invert` flips it.
pub fn signed(rpm: u32, invert: bool) -> f32 {
    let speed = rpm as f32;
    if invert { -speed } else { speed }
}

/// Reads a motor's settings from its state properties: the preset speed and whether it is
/// inverted. Missing or unknown values fall back to the default speed and no inversion.
pub fn read(property: impl Fn(&str) -> Option<String>) -> (u32, bool) {
    let rpm = property("rpm")
        .as_deref()
        .and_then(from_name)
        .unwrap_or(DEFAULT_RPM);
    let invert = property("invert").as_deref() == Some("true");
    (rpm, invert)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_powers_of_two_from_one_to_256() {
        assert_eq!(PRESETS.first(), Some(&1));
        assert_eq!(PRESETS.last(), Some(&256));
        for pair in PRESETS.windows(2) {
            assert_eq!(pair[1], pair[0] * 2);
        }
        assert!(PRESETS.contains(&DEFAULT_RPM));
    }

    #[test]
    fn names_round_trip_for_every_preset() {
        for preset in PRESETS {
            assert_eq!(from_name(name(preset)), Some(preset));
        }
        // Zero is a displayed level but never a motor setting.
        assert_eq!(from_name("0"), None);
        assert_eq!(from_name("3"), None);
        assert_eq!(from_name("fast"), None);
    }

    #[test]
    fn stepping_goes_up_and_wraps_to_the_slowest() {
        assert_eq!(next(1), 2);
        assert_eq!(next(16), 32);
        assert_eq!(next(128), 256);
        assert_eq!(next(256), 1);
    }

    #[test]
    fn stepping_nine_times_comes_back_to_the_start() {
        for start in PRESETS {
            let mut rpm = start;
            for _ in 0..PRESETS.len() {
                rpm = next(rpm);
            }
            assert_eq!(rpm, start);
        }
    }

    #[test]
    fn an_unknown_speed_steps_to_the_default() {
        assert_eq!(next(0), DEFAULT_RPM);
        assert_eq!(next(24), DEFAULT_RPM);
    }

    #[test]
    fn the_click_rises_with_each_preset_and_stays_in_the_game_range() {
        let pitches: Vec<f32> = PRESETS.iter().map(|rpm| click_pitch(*rpm)).collect();
        assert!(pitches.windows(2).all(|pair| pair[1] > pair[0]), "{pitches:?}");
        // The game clamps pitch to 0.5 .. 2.0.
        assert!(pitches.iter().all(|pitch| (0.5..=2.0).contains(pitch)));
        // An unknown speed gets a middle pitch rather than failing.
        assert_eq!(click_pitch(3), click_pitch(DEFAULT_RPM));
    }

    #[test]
    fn inversion_flips_the_sign_only() {
        assert_eq!(signed(32, false), 32.0);
        assert_eq!(signed(32, true), -32.0);
        assert_eq!(signed(1, true), -1.0);
    }

    #[test]
    fn settings_are_read_from_state_properties() {
        let state = |rpm: &'static str, invert: &'static str| {
            move |name: &str| match name {
                "rpm" => Some(rpm.to_string()),
                "invert" => Some(invert.to_string()),
                _ => None,
            }
        };
        assert_eq!(read(state("64", "false")), (64, false));
        assert_eq!(read(state("8", "true")), (8, true));
        // A missing or broken value falls back to the default.
        assert_eq!(read(|_| None), (DEFAULT_RPM, false));
        assert_eq!(read(state("999", "maybe")), (DEFAULT_RPM, false));
    }
}

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

/// How many positions the speed slider has: every preset, once backwards and once forwards.
pub const SLIDER_STEPS: usize = PRESETS.len() * 2;

/// The signed speed at each slider position, from the fastest backwards to the fastest forwards:
/// -256 ... -1, then 1 ... 256. There is no zero: a motor always turns, and its direction is the
/// side of the slider it is on.
pub fn slider_values() -> [i32; SLIDER_STEPS] {
    let mut values = [0; SLIDER_STEPS];
    let n = PRESETS.len();
    for (i, preset) in PRESETS.iter().enumerate() {
        values[n - 1 - i] = -(*preset as i32);
        values[n + i] = *preset as i32;
    }
    values
}

/// The signed speed of a motor set to `rpm` and `invert`, as a whole number.
pub fn signed_int(rpm: u32, invert: bool) -> i32 {
    let speed = rpm as i32;
    if invert { -speed } else { speed }
}

/// The preset speed and direction that a signed speed stands for: its size, and whether it is
/// negative. A speed that is not a preset is rounded to the nearest slider position first.
pub fn split(signed: i32) -> (u32, bool) {
    let values = slider_values();
    let value = values[slider_index(signed)];
    (value.unsigned_abs(), value < 0)
}

/// The slider position nearest to a signed speed; zero goes to the slowest forwards position.
pub fn slider_index(signed: i32) -> usize {
    let values = slider_values();
    let mut best = 0;
    for (index, value) in values.iter().enumerate() {
        let distance = (value - signed).abs();
        let best_distance = (values[best] - signed).abs();
        // On a tie the later (more forwards) position wins, so zero lands on +1.
        if distance <= best_distance {
            best = index;
        }
    }
    best
}

/// The signed speed one slider position away from `signed`, `delta` positions forwards (or
/// backwards when negative), stopping at the ends.
pub fn step_by(signed: i32, delta: i32) -> i32 {
    let values = slider_values();
    let index = slider_index(signed) as i32 + delta;
    values[index.clamp(0, SLIDER_STEPS as i32 - 1) as usize]
}

/// The x of slider position `index` on a track that starts at `left` and is `width` wide: the
/// positions are spread evenly with the first on the left end and the last on the right end.
pub fn slider_x(index: usize, left: f32, width: f32) -> f32 {
    left + width * index as f32 / (SLIDER_STEPS - 1) as f32
}

/// The slider position a press at `x` picks on that track, the nearest one, or `None` when `x` is
/// outside the track by more than half a gap.
pub fn index_at(x: f32, left: f32, width: f32) -> Option<usize> {
    let gap = width / (SLIDER_STEPS - 1) as f32;
    if x < left - gap / 2.0 || x > left + width + gap / 2.0 {
        return None;
    }
    let index = ((x - left) / gap).round().clamp(0.0, (SLIDER_STEPS - 1) as f32);
    Some(index as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slider_runs_from_the_fastest_backwards_to_the_fastest_forwards() {
        let values = slider_values();
        assert_eq!(values.len(), SLIDER_STEPS);
        assert_eq!(values[0], -256);
        assert_eq!(values[8], -1);
        assert_eq!(values[9], 1);
        assert_eq!(values[SLIDER_STEPS - 1], 256);
        assert!(values.windows(2).all(|pair| pair[0] < pair[1]), "{values:?}");
        assert!(!values.contains(&0));
    }

    #[test]
    fn every_slider_position_splits_into_a_preset_and_a_direction() {
        for value in slider_values() {
            let (rpm, invert) = split(value);
            assert!(PRESETS.contains(&rpm), "{value}");
            assert_eq!(invert, value < 0);
            assert_eq!(signed_int(rpm, invert), value);
        }
    }

    #[test]
    fn a_speed_off_the_presets_rounds_to_the_nearest_position() {
        assert_eq!(split(20), (16, false));
        assert_eq!(split(-100), (128, true));
        assert_eq!(split(1000), (256, false));
        assert_eq!(split(-1000), (256, true));
        // Zero is not a speed: it goes to the slowest forwards position.
        assert_eq!(split(0), (1, false));
    }

    #[test]
    fn stepping_the_slider_moves_one_position_and_stops_at_the_ends() {
        assert_eq!(step_by(16, 1), 32);
        assert_eq!(step_by(16, -1), 8);
        // Crossing the middle changes direction: from 1 one step back is -1.
        assert_eq!(step_by(1, -1), -1);
        assert_eq!(step_by(-1, 1), 1);
        assert_eq!(step_by(256, 1), 256);
        assert_eq!(step_by(-256, -1), -256);
    }

    #[test]
    fn positions_are_spread_evenly_across_the_track() {
        assert_eq!(slider_x(0, 12.0, 170.0), 12.0);
        assert_eq!(slider_x(SLIDER_STEPS - 1, 12.0, 170.0), 182.0);
        let gap = slider_x(1, 12.0, 170.0) - slider_x(0, 12.0, 170.0);
        assert!((slider_x(9, 12.0, 170.0) - slider_x(8, 12.0, 170.0) - gap).abs() < 1e-4);
    }

    #[test]
    fn a_press_picks_the_nearest_position_and_ignores_what_is_far_off_the_track() {
        for index in 0..SLIDER_STEPS {
            let x = slider_x(index, 12.0, 152.0);
            assert_eq!(index_at(x, 12.0, 152.0), Some(index));
        }
        assert_eq!(index_at(0.0, 12.0, 152.0), None);
        assert_eq!(index_at(200.0, 12.0, 152.0), None);
        // A press just past an end still counts as the end.
        assert_eq!(index_at(10.0, 12.0, 152.0), Some(0));
        assert_eq!(index_at(166.0, 12.0, 152.0), Some(SLIDER_STEPS - 1));
    }

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

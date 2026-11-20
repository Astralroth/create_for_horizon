//! How a block's speed is shown: pure helpers with no game types.
//!
//! The server stores a block's speed in two state properties so that every client gets it with
//! the block: `speed`, the closest power of two from 1 to 256 (or 0 when stopped), and
//! `reverse`, whether it turns the other way. The client turns the block's own model by an angle
//! worked out from them and the animation clock.

/// The displayed speeds, in RPM.
pub const LEVELS: [u32; 10] = [0, 1, 2, 4, 8, 16, 32, 64, 128, 256];

/// Degrees a block at 1 RPM turns in one game tick: 360 degrees per minute, 1200 ticks.
const DEGREES_PER_TICK_PER_RPM: f64 = 360.0 / 60.0 / 20.0;

/// The displayed level closest to `rpm` on a logarithmic scale: 0 for a stopped block.
pub fn level(rpm: f32) -> u32 {
    let speed = rpm.abs();
    if !speed.is_finite() || speed < 0.5 {
        return 0;
    }
    1 << speed.log2().round().clamp(0.0, 8.0) as u32
}

/// The state value that spells `level`.
pub fn level_name(level: u32) -> &'static str {
    match level {
        0 => "0",
        1 => "1",
        2 => "2",
        4 => "4",
        8 => "8",
        16 => "16",
        32 => "32",
        64 => "64",
        128 => "128",
        _ => "256",
    }
}

/// The level a state value spells, or `None` for anything else.
pub fn level_from_name(name: &str) -> Option<u32> {
    LEVELS.into_iter().find(|level| level_name(*level) == name)
}

/// The angle, in degrees counterclockwise seen from the positive end of the axis, a block at
/// `level` has after `ticks` of the animation clock. A reversed block turns the other way.
pub fn degrees(level: u32, reverse: bool, ticks: f64) -> f32 {
    let angle = (f64::from(level) * DEGREES_PER_TICK_PER_RPM * ticks) % 360.0;
    (if reverse { -angle } else { angle }) as f32
}

/// How many steps a gauge's dial has above zero: the dial runs from 0 to this value.
pub const DIAL_STEPS: u32 = 16;

/// The dial step a speedometer shows for a block at displayed speed `level`: 0 when stopped,
/// then 1 for 1 RPM up to the last step for 256 RPM, on a logarithmic scale.
pub fn speed_dial(level: u32) -> u32 {
    if level == 0 {
        return 0;
    }
    let steps = (level as f32).log2() * (DIAL_STEPS - 1) as f32 / 8.0;
    (1 + steps.round() as u32).min(DIAL_STEPS)
}

/// The dial step a stressometer shows: the share of the capacity that is in use. A network
/// stopped by too much stress shows a full dial, and one with no capacity shows none.
pub fn stress_dial(stress: f32, capacity: f32, overstressed: bool) -> u32 {
    if overstressed {
        return DIAL_STEPS;
    }
    if !capacity.is_finite() || capacity <= 0.0 || !stress.is_finite() || stress <= 0.0 {
        return 0;
    }
    ((stress / capacity).clamp(0.0, 1.0) * DIAL_STEPS as f32).round() as u32
}

/// The state value that spells a dial step.
pub fn dial_name(dial: u32) -> String {
    dial.min(DIAL_STEPS).to_string()
}

/// The dial step a state value spells.
pub fn dial_from_name(name: &str) -> Option<u32> {
    name.parse().ok().filter(|dial| *dial <= DIAL_STEPS)
}

/// The angle, in degrees counterclockwise seen from the positive end of its axis, a gauge's needle
/// is turned by at `dial`. The needle sweeps clockwise as seen from the front, from 135 degrees
/// left of straight up at 0 to 135 degrees right of it at the last step. `facing_positive` says
/// whether the gauge faces a positive direction, which decides which end the viewer is at.
pub fn needle_degrees(dial: u32, facing_positive: bool) -> f32 {
    let clockwise = -135.0 + 270.0 * dial.min(DIAL_STEPS) as f32 / DIAL_STEPS as f32;
    if facing_positive { -clockwise } else { clockwise }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_speedometer_dial_runs_from_zero_to_full_on_a_log_scale() {
        assert_eq!(speed_dial(0), 0);
        assert_eq!(speed_dial(1), 1);
        assert_eq!(speed_dial(256), DIAL_STEPS);
        // Each doubling adds a step of nearly two.
        let steps: Vec<u32> = [1, 2, 4, 8, 16, 32, 64, 128, 256]
            .into_iter()
            .map(speed_dial)
            .collect();
        assert!(steps.windows(2).all(|pair| pair[1] > pair[0]), "{steps:?}");
        assert_eq!(speed_dial(16), 9);
    }

    #[test]
    fn a_stressometer_dial_shows_the_share_in_use() {
        assert_eq!(stress_dial(0.0, 1000.0, false), 0);
        assert_eq!(stress_dial(500.0, 1000.0, false), 8);
        assert_eq!(stress_dial(1000.0, 1000.0, false), DIAL_STEPS);
        assert_eq!(stress_dial(5000.0, 1000.0, false), DIAL_STEPS);
        assert_eq!(stress_dial(100.0, 1000.0, true), DIAL_STEPS);
        // No capacity or unusable numbers show an empty dial rather than failing.
        assert_eq!(stress_dial(10.0, 0.0, false), 0);
        assert_eq!(stress_dial(f32::NAN, 1000.0, false), 0);
        assert_eq!(stress_dial(10.0, f32::INFINITY, false), 0);
    }

    #[test]
    fn dial_names_round_trip_and_reject_out_of_range() {
        for dial in 0..=DIAL_STEPS {
            assert_eq!(dial_from_name(&dial_name(dial)), Some(dial));
        }
        assert_eq!(dial_from_name("17"), None);
        assert_eq!(dial_from_name("-1"), None);
        assert_eq!(dial_from_name("half"), None);
        assert_eq!(dial_name(99), "16");
    }

    #[test]
    fn the_needle_sweeps_clockwise_as_seen_from_the_front() {
        // Seen from a negative-facing gauge the world sign is the clockwise angle itself, and
        // from a positive-facing one it is the opposite.
        assert_eq!(needle_degrees(0, false), -135.0);
        assert_eq!(needle_degrees(DIAL_STEPS, false), 135.0);
        assert_eq!(needle_degrees(DIAL_STEPS / 2, false), 0.0);
        for dial in 0..=DIAL_STEPS {
            assert_eq!(needle_degrees(dial, true), -needle_degrees(dial, false));
        }
        // The needle never goes past the ends of the dial.
        assert_eq!(needle_degrees(500, false), 135.0);
    }

    #[test]
    fn stopped_blocks_have_level_zero() {
        assert_eq!(level(0.0), 0);
        assert_eq!(level(0.4), 0);
        assert_eq!(level(-0.4), 0);
        assert_eq!(level(f32::NAN), 0);
    }

    #[test]
    fn exact_powers_of_two_keep_their_level() {
        for level_value in [1, 2, 4, 8, 16, 32, 64, 128, 256] {
            assert_eq!(level(level_value as f32), level_value);
            assert_eq!(level(-(level_value as f32)), level_value);
        }
    }

    #[test]
    fn other_speeds_round_to_the_closest_level() {
        // The border between 16 and 32 is their geometric mean, about 22.6 RPM.
        assert_eq!(level(24.0), 32);
        assert_eq!(level(23.0), 32);
        assert_eq!(level(22.0), 16);
        assert_eq!(level(100.0), 128);
        assert_eq!(level(1000.0), 256);
        assert_eq!(level(0.6), 1);
    }

    #[test]
    fn names_round_trip() {
        for level_value in LEVELS {
            assert_eq!(level_from_name(level_name(level_value)), Some(level_value));
        }
        assert_eq!(level_from_name("3"), None);
        assert_eq!(level_from_name("fast"), None);
    }

    #[test]
    fn a_level_turns_six_degrees_per_second_for_each_rpm() {
        // 64 RPM is 384 degrees in the 20 ticks of a second: one turn and 24 degrees.
        assert!((degrees(64, false, 20.0) - 24.0).abs() < 1e-3);
        // 1 RPM turns 6 degrees per second, 0.3 per tick.
        assert!((degrees(1, false, 10.0) - 3.0).abs() < 1e-4);
    }

    #[test]
    fn reversed_blocks_turn_the_other_way_and_stopped_ones_do_not_move() {
        assert!((degrees(4, true, 10.0) + degrees(4, false, 10.0)).abs() < 1e-4);
        assert_eq!(degrees(0, false, 1234.5), 0.0);
        assert_eq!(degrees(0, true, 1234.5), 0.0);
    }

    #[test]
    fn the_angle_stays_within_one_turn() {
        for ticks in [0.0, 1.0, 1e3, 1e6, 123_456.789] {
            let value = degrees(256, false, ticks);
            assert!((-360.0..360.0).contains(&value), "{value}");
        }
    }
}

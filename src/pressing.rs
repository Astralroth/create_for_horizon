//! The rules of the Mechanical Press with no game types: its stress, how long a recipe takes and
//! how far its ram goes down while it works. The adapter in `press.rs` reads the world.

/// Stress units the press uses for every RPM it turns at.
pub const IMPACT: f32 = 8.0;
/// Work, in RPM-ticks, of a recipe that does not say how much it needs.
pub const DEFAULT_WORK: f32 = 300.0;
/// The most work a recipe may ask for, so a typo cannot make a press that never finishes.
pub const MAX_WORK: f32 = 100_000.0;
/// How far the ram goes down at the bottom of its stroke, in blocks.
pub const STROKE: f32 = 0.85;
/// Game ticks a full stroke takes at 16 RPM.
const CYCLE_TICKS_AT_16: f64 = 40.0;
/// The shortest and longest a stroke takes, in ticks.
const CYCLE_RANGE: (f64, f64) = (8.0, 160.0);

/// The work a recipe needs, from its `work` field: the default when it is missing or not a
/// positive number, and never more than [`MAX_WORK`].
pub fn work_of(field: Option<f64>) -> f32 {
    match field {
        Some(work) if work.is_finite() && work >= 1.0 => (work as f32).min(MAX_WORK),
        _ => DEFAULT_WORK,
    }
}

/// How long one stroke of the ram takes, in ticks, at a displayed speed level (1 to 256 RPM):
/// faster presses stroke faster, within limits.
pub fn cycle_ticks(level: u32) -> f64 {
    let level = f64::from(level.max(1));
    (CYCLE_TICKS_AT_16 * 16.0 / level).clamp(CYCLE_RANGE.0, CYCLE_RANGE.1)
}

/// How far down the ram is, in blocks, at `ticks` of the animation clock for a press at `level`:
/// it starts at the top, goes down and comes back smoothly, over and over.
pub fn ram_depth(ticks: f64, level: u32) -> f32 {
    let cycle = cycle_ticks(level);
    let phase = (ticks.rem_euclid(cycle)) / cycle;
    let smooth = (1.0 - (phase * std::f64::consts::TAU).cos()) / 2.0;
    STROKE * smooth as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_without_a_usable_work_value_takes_the_default() {
        assert_eq!(work_of(None), DEFAULT_WORK);
        assert_eq!(work_of(Some(0.0)), DEFAULT_WORK);
        assert_eq!(work_of(Some(-5.0)), DEFAULT_WORK);
        assert_eq!(work_of(Some(f64::NAN)), DEFAULT_WORK);
        assert_eq!(work_of(Some(f64::INFINITY)), DEFAULT_WORK);
    }

    #[test]
    fn a_recipe_work_value_is_used_and_capped() {
        assert_eq!(work_of(Some(200.0)), 200.0);
        assert_eq!(work_of(Some(1.0)), 1.0);
        assert_eq!(work_of(Some(1.0e9)), MAX_WORK);
    }

    #[test]
    fn faster_presses_stroke_faster_within_limits() {
        assert_eq!(cycle_ticks(16), 40.0);
        assert!(cycle_ticks(32) < cycle_ticks(16));
        assert!(cycle_ticks(8) > cycle_ticks(16));
        assert_eq!(cycle_ticks(256), CYCLE_RANGE.0);
        assert_eq!(cycle_ticks(1), CYCLE_RANGE.1);
        // A stopped press, level 0, is read as the slowest rather than dividing by zero.
        assert_eq!(cycle_ticks(0), CYCLE_RANGE.1);
    }

    #[test]
    fn the_ram_starts_at_the_top_reaches_the_bottom_halfway_and_comes_back() {
        assert!(ram_depth(0.0, 16).abs() < 1e-6);
        assert!((ram_depth(20.0, 16) - STROKE).abs() < 1e-6);
        assert!(ram_depth(40.0, 16).abs() < 1e-5);
    }

    #[test]
    fn the_ram_never_leaves_its_stroke_and_repeats_every_cycle() {
        for tick in 0..400 {
            let depth = ram_depth(f64::from(tick) * 0.7, 16);
            assert!((0.0..=STROKE + 1e-6).contains(&depth), "tick {tick}: {depth}");
        }
        let a = ram_depth(7.3, 16);
        let b = ram_depth(7.3 + 40.0, 16);
        assert!((a - b).abs() < 1e-5);
    }
}

//! The rules of the power sources other than the Creative Motor: the Hand Crank and the Water
//! Wheel. Pure logic with no game types; the adapter reads the world and calls these.

/// How many steps of charge a click gives the crank. It lets go of one step every
/// [`CHARGE_STEP_TICKS`], so a click keeps it turning for a short burst and holding the use
/// button (which repeats the click) keeps it turning for as long as it is held.
pub const CHARGE_MAX: u8 = 4;
/// Game ticks between one charge step and the next.
pub const CHARGE_STEP_TICKS: i64 = 5;
/// The speed of a charged crank.
pub const CRANK_RPM: f32 = 32.0;
/// Stress units a source carries for every RPM it turns at: the crank and the wheel are small
/// sources, a quarter of the Creative Motor's 1024.
pub const CAPACITY_PER_RPM: f32 = 256.0;
/// RPM the wheel gains for every side with flowing water against it.
pub const WHEEL_RPM_PER_SIDE: f32 = 4.0;
/// The most sides of a wheel that can have water: the four across its axis.
pub const WHEEL_SIDES: usize = 4;

/// The state value of a charge: "0" to "4".
pub fn charge_name(charge: u8) -> &'static str {
    match charge {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        _ => "4",
    }
}

/// The charge a state value stands for.
pub fn charge_from(name: &str) -> Option<u8> {
    name.parse::<u8>().ok().filter(|charge| *charge <= CHARGE_MAX)
}

/// The charge one step later.
pub fn cooled(charge: u8) -> u8 {
    charge.saturating_sub(1)
}

/// The signed speed of a crank with this charge: it turns at full speed while it has any.
pub fn crank_rpm(charge: u8) -> f32 {
    if charge == 0 { 0.0 } else { CRANK_RPM }
}

/// How much the water in the cell at `offset` from a wheel turns it about `axis`: the component
/// of the torque `offset x flow` along the axis. Positive is counterclockwise seen from the
/// positive end of the axis, the convention of every shaft in a network. Water on opposite sides
/// of the wheel flowing the same way gives opposite signs, so it cancels.
pub fn turning(axis: [i32; 3], offset: [i32; 3], flow: [f64; 3]) -> f64 {
    let r = offset.map(f64::from);
    let torque = [
        r[1] * flow[2] - r[2] * flow[1],
        r[2] * flow[0] - r[0] * flow[2],
        r[0] * flow[1] - r[1] * flow[0],
    ];
    (0..3).map(|i| f64::from(axis[i]) * torque[i]).sum()
}

/// What one side's water does to the wheel: `1` or `-1` when it turns it clearly one way, `0`
/// when it hardly pushes at all (still water, or a flow straight at the axle).
pub fn push(turning: f64) -> i32 {
    if turning > 0.1 {
        1
    } else if turning < -0.1 {
        -1
    } else {
        0
    }
}

/// The signed speed of a wheel whose sides push it by `net` in all (each side counts one, and
/// sides pushing opposite ways cancel), turned around when `invert` is set.
pub fn wheel_rpm(net: i32, invert: bool) -> f32 {
    let rpm = net.unsigned_abs().min(WHEEL_SIDES as u32) as f32 * WHEEL_RPM_PER_SIDE;
    let rpm = if net < 0 { -rpm } else { rpm };
    if invert { -rpm } else { rpm }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_names_round_trip() {
        for charge in 0..=CHARGE_MAX {
            assert_eq!(charge_from(charge_name(charge)), Some(charge));
        }
        assert_eq!(charge_from("5"), None);
        assert_eq!(charge_from("-1"), None);
        assert_eq!(charge_from("full"), None);
    }

    #[test]
    fn a_charge_wears_off_one_step_at_a_time_and_stops_at_zero() {
        let mut charge = CHARGE_MAX;
        let mut steps = 0;
        while charge > 0 {
            charge = cooled(charge);
            steps += 1;
        }
        assert_eq!(steps, CHARGE_MAX);
        assert_eq!(cooled(0), 0);
    }

    #[test]
    fn a_click_keeps_the_crank_turning_for_more_than_one_refresh() {
        // The network looks at a block every 10 ticks, so a lone click must outlast that.
        assert!(i64::from(CHARGE_MAX) * CHARGE_STEP_TICKS >= 10);
    }

    #[test]
    fn a_crank_turns_at_full_speed_while_charged_and_not_at_all_otherwise() {
        assert_eq!(crank_rpm(0), 0.0);
        for charge in 1..=CHARGE_MAX {
            assert_eq!(crank_rpm(charge), CRANK_RPM);
        }
    }

    #[test]
    fn a_wheel_is_as_fast_as_the_net_push_of_its_sides() {
        assert_eq!(wheel_rpm(0, false), 0.0);
        assert_eq!(wheel_rpm(1, false), 4.0);
        assert_eq!(wheel_rpm(2, false), 8.0);
        assert_eq!(wheel_rpm(4, false), 16.0);
        assert_eq!(wheel_rpm(-2, false), -8.0);
        // There is no fifth side.
        assert_eq!(wheel_rpm(9, false), 16.0);
    }

    const X: [i32; 3] = [1, 0, 0];

    #[test]
    fn a_stream_over_the_top_and_one_under_the_bottom_flowing_the_same_way_cancel() {
        // A wheel turning about x with water above (offset +y) and below (-y), both flowing
        // south (+z): one stream pushes it each way, so they act against each other.
        let above = turning(X, [0, 1, 0], [0.0, 0.0, 1.0]);
        let below = turning(X, [0, -1, 0], [0.0, 0.0, 1.0]);
        assert_eq!(push(above), -push(below));
        assert_ne!(push(above), 0);
        assert_eq!(push(above) + push(below), 0);
    }

    #[test]
    fn streams_that_flow_opposite_ways_on_opposite_sides_add_up() {
        // Over the top flowing south and under the bottom flowing north is what a real wheel
        // in a current feels: both push it the same way.
        let above = turning(X, [0, 1, 0], [0.0, 0.0, 1.0]);
        let below = turning(X, [0, -1, 0], [0.0, 0.0, -1.0]);
        assert_eq!(push(above), push(below));
        assert_eq!(push(above) + push(below), 2 * push(above));
    }

    #[test]
    fn a_stream_flowing_toward_the_axle_or_along_it_does_not_turn_the_wheel() {
        // Water above flowing straight down at the wheel, or along its axis.
        assert_eq!(push(turning(X, [0, 1, 0], [0.0, -1.0, 0.0])), 0);
        assert_eq!(push(turning(X, [0, 1, 0], [1.0, 0.0, 0.0])), 0);
        // Still water has no flow.
        assert_eq!(push(turning(X, [0, 1, 0], [0.0, 0.0, 0.0])), 0);
    }

    #[test]
    fn a_falling_stream_on_one_side_turns_the_wheel_and_on_both_sides_stops_it() {
        // Water falling past the +z side and past the -z side of a wheel about x.
        let one = turning(X, [0, 0, 1], [0.0, -1.0, 0.0]);
        let other = turning(X, [0, 0, -1], [0.0, -1.0, 0.0]);
        assert_ne!(push(one), 0);
        assert_eq!(push(one) + push(other), 0);
    }

    #[test]
    fn the_turning_sign_follows_the_counterclockwise_convention() {
        // Seen from +x looking back at the origin, y points up and z points to the left, so
        // water above flowing toward +z moves counterclockwise.
        assert!(turning(X, [0, 1, 0], [0.0, 0.0, 1.0]) > 0.0);
        assert!(turning(X, [0, 1, 0], [0.0, 0.0, -1.0]) < 0.0);
    }

    #[test]
    fn inverting_a_wheel_turns_it_the_other_way_but_not_faster() {
        assert_eq!(wheel_rpm(3, true), -wheel_rpm(3, false));
        assert_eq!(wheel_rpm(-3, true), 12.0);
        // Zero stays zero, with no negative zero to confuse a comparison.
        assert_eq!(wheel_rpm(0, true), 0.0);
    }
}

//! The Belt's rules with no game types: how far each item has got, when it stops behind the one
//! ahead, when it may leave and when a new one may get on.
//!
//! Positions are *progress along the flow*: 0 where an item gets on and 1 where it leaves, whichever
//! way the belt turns. A rider is stored as where it was (`p`) at a tick (`t0`), and moves at the
//! belt's rate from then on, so nothing has to be written every tick: only when something
//! changes (a rider stops, starts, gets on, leaves, or the belt's speed changes). The server and
//! the clients work out the same positions from the same stored values.
//!
//! Riders are kept front first: the one closest to the exit is first.

/// The closest two riders may be, in belt lengths.
pub const GAP: f32 = 0.25;
/// How many riders one belt piece carries: its length over the gap.
pub const MAX_RIDERS: usize = 4;
/// Belt lengths a rider moves each tick for each RPM, so 16 RPM is a tenth of a length a tick
/// (two blocks a second).
pub const PROGRESS_PER_TICK_PER_RPM: f32 = 0.00625;
/// The speed above which a belt does not carry items any faster, in RPM.
pub const MAX_RPM: i32 = 64;
/// Stress units the belt uses for every RPM it turns at.
pub const IMPACT: f32 = 2.0;
/// Tolerance when comparing positions.
const EPSILON: f32 = 1.0e-4;

/// One item on the belt: where it was, when, and whether it is held up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rider {
    /// Progress at tick `t0`, 0 to 1.
    pub p: f32,
    /// The tick `p` was written, with the frame's partial tick on a client.
    pub t0: f64,
    /// Held up by the end of the belt or by the rider ahead: it does not move.
    pub stopped: bool,
}

/// The belt lengths a rider moves each tick at a signed speed in RPM: never negative, and capped.
pub fn rate_for(signed_rpm: i32) -> f32 {
    signed_rpm.abs().min(MAX_RPM) as f32 * PROGRESS_PER_TICK_PER_RPM
}

/// Which way a signed speed carries items: `1` forwards, `-1` backwards, `0` when it is stopped.
pub fn flow_sign(signed_rpm: i32) -> i32 {
    signed_rpm.signum()
}

/// Where each rider is at `now`: moving ones from where they were, stopped ones where they are,
/// none closer than [`GAP`] to the one ahead or past the exit.
pub fn positions(riders: &[Rider], rate: f32, now: f64) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::with_capacity(riders.len());
    for (index, rider) in riders.iter().enumerate() {
        let raw = if rider.stopped {
            rider.p
        } else {
            rider.p + rate * (now - rider.t0) as f32
        };
        let limit = match index {
            0 => 1.0,
            _ => out[index - 1] - GAP,
        };
        out.push(raw.min(limit));
    }
    out
}

/// Whether a new rider may get on at `now`: there is a free place and no rider is within the gap
/// of the entrance.
pub fn can_accept(riders: &[Rider], rate: f32, now: f64) -> bool {
    if riders.len() >= MAX_RIDERS {
        return false;
    }
    positions(riders, rate, now)
        .last()
        .is_none_or(|last| *last >= GAP - EPSILON)
}

/// Puts a new rider at the entrance, behind all the others.
pub fn push(riders: &mut Vec<Rider>, now: f64) {
    riders.push(Rider {
        p: 0.0,
        t0: now,
        stopped: false,
    });
}

/// Rewrites every moving rider as where it is now, so the rate can change from here on.
pub fn rebase(riders: &mut [Rider], rate: f32, now: f64) {
    let at = positions(riders, rate, now);
    for (rider, here) in riders.iter_mut().zip(at) {
        if !rider.stopped {
            rider.p = here;
            rider.t0 = now;
        }
    }
}

/// Turns the flow around: a rider that had `p` of the way now has the rest, and the one that was
/// last is first. Nobody is held up any more; the next tick finds out who is.
pub fn flip(riders: &mut Vec<Rider>, now: f64) {
    for rider in riders.iter_mut() {
        rider.p = 1.0 - rider.p;
        rider.t0 = now;
        rider.stopped = false;
    }
    riders.reverse();
}

/// What a tick found out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Something that is stored changed, so the belt must be saved and the clients told.
    pub changed: bool,
    /// The front rider got off: it was removed.
    pub left: bool,
}

/// One tick: stops the riders that are held up, starts those that are free again, and lets the
/// front rider off when it reaches the exit and `leave` says something took it.
pub fn step(riders: &mut Vec<Rider>, rate: f32, now: f64, mut leave: impl FnMut() -> bool) -> Step {
    let mut changed = false;
    let mut previous: Option<f32> = None;
    for index in 0..riders.len() {
        let rider = riders[index];
        let raw = if rider.stopped {
            rider.p
        } else {
            rider.p + rate * (now - rider.t0) as f32
        };
        let limit = match previous {
            None => 1.0,
            Some(ahead) => ahead - GAP,
        };
        let blocked = raw >= limit - EPSILON;
        if index == 0 && raw >= 1.0 - EPSILON && rate > 0.0 && leave() {
            riders.remove(0);
            return Step {
                changed: true,
                left: true,
            };
        }
        let here = raw.min(limit);
        if blocked != rider.stopped {
            riders[index] = Rider {
                p: here,
                t0: now,
                stopped: blocked,
            };
            changed = true;
        }
        previous = Some(here);
    }
    Step {
        changed,
        left: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 0.1;

    fn rider(p: f32, t0: f64) -> Rider {
        Rider {
            p,
            t0,
            stopped: false,
        }
    }

    #[test]
    fn the_rate_follows_the_speed_and_is_capped_and_unsigned() {
        assert!((rate_for(16) - 0.1).abs() < 1e-6);
        assert_eq!(rate_for(-16), rate_for(16));
        assert_eq!(rate_for(0), 0.0);
        assert_eq!(rate_for(256), rate_for(MAX_RPM));
        assert!(rate_for(MAX_RPM) < 1.0, "an item must not skip a whole piece in a tick");
    }

    #[test]
    fn the_flow_sign_is_the_sign_of_the_speed() {
        assert_eq!(flow_sign(16), 1);
        assert_eq!(flow_sign(-1), -1);
        assert_eq!(flow_sign(0), 0);
    }

    #[test]
    fn a_rider_moves_at_the_rate_and_stops_at_the_exit() {
        let riders = [rider(0.0, 0.0)];
        assert!((positions(&riders, RATE, 5.0)[0] - 0.5).abs() < 1e-6);
        assert_eq!(positions(&riders, RATE, 50.0)[0], 1.0);
    }

    #[test]
    fn a_stopped_rider_does_not_move() {
        let riders = [Rider {
            p: 0.4,
            t0: 0.0,
            stopped: true,
        }];
        assert_eq!(positions(&riders, RATE, 100.0)[0], 0.4);
    }

    #[test]
    fn a_rider_behind_never_gets_closer_than_the_gap() {
        let riders = [
            Rider {
                p: 1.0,
                t0: 0.0,
                stopped: true,
            },
            rider(0.0, 0.0),
        ];
        let at = positions(&riders, RATE, 100.0);
        assert_eq!(at[0], 1.0);
        assert!((at[1] - (1.0 - GAP)).abs() < 1e-6);
    }

    #[test]
    fn nobody_gets_on_when_the_entrance_is_taken_or_the_belt_is_full() {
        // A rider just got on: the entrance is not clear until it has moved a gap.
        let riders = [rider(0.0, 10.0)];
        assert!(!can_accept(&riders, RATE, 10.0));
        assert!(can_accept(&riders, RATE, 10.0 + f64::from(GAP / RATE)));
        assert!(can_accept(&[], RATE, 0.0));
        // Four held-up riders fill the piece.
        let full: Vec<Rider> = (0..MAX_RIDERS)
            .map(|i| Rider {
                p: 1.0 - GAP * i as f32,
                t0: 0.0,
                stopped: true,
            })
            .collect();
        assert!(!can_accept(&full, RATE, 0.0));
    }

    #[test]
    fn a_belt_carries_exactly_as_many_riders_as_fit_in_its_length() {
        assert_eq!(MAX_RIDERS as f32 * GAP, 1.0);
    }

    #[test]
    fn the_front_rider_leaves_at_the_exit_when_something_takes_it() {
        let mut riders = vec![rider(0.0, 0.0)];
        let step = step(&mut riders, RATE, 11.0, || true);
        assert!(step.left && step.changed);
        assert!(riders.is_empty());
    }

    #[test]
    fn a_rider_nobody_takes_waits_at_the_exit_and_is_marked_stopped() {
        let mut riders = vec![rider(0.0, 0.0)];
        let first = step(&mut riders, RATE, 11.0, || false);
        assert!(first.changed && !first.left);
        assert_eq!(
            riders[0],
            Rider {
                p: 1.0,
                t0: 11.0,
                stopped: true
            }
        );
        // The next tick finds nothing new to write.
        let again = step(&mut riders, RATE, 12.0, || false);
        assert!(!again.changed && !again.left);
    }

    #[test]
    fn the_exit_is_asked_about_only_when_the_belt_is_moving() {
        let mut riders = vec![rider(1.0, 0.0)];
        let mut asked = false;
        let outcome = step(&mut riders, 0.0, 5.0, || {
            asked = true;
            true
        });
        assert!(!asked && !outcome.left);
    }

    #[test]
    fn a_follower_stops_behind_a_held_up_rider_and_starts_again_when_it_leaves() {
        let mut riders = vec![rider(0.0, 0.0), rider(0.0, -3.0)];
        // The front reaches the exit at tick 10 and nothing takes it; the follower, already past
        // where the gap allows, is held a gap behind it in the same tick.
        let first = step(&mut riders, RATE, 11.0, || false);
        assert!(first.changed);
        assert!(riders[0].stopped && riders[1].stopped);
        assert!((riders[1].p - (1.0 - GAP)).abs() < 1e-5);
        // Nothing more to write while they wait.
        assert!(!step(&mut riders, RATE, 20.0, || false).changed);
        // Now the front rider is taken: the follower is free and moves on.
        let out = step(&mut riders, RATE, 21.0, || true);
        assert!(out.left);
        assert_eq!(riders.len(), 1);
        let next = step(&mut riders, RATE, 22.0, || false);
        assert!(next.changed && !riders[0].stopped);
    }

    #[test]
    fn a_belt_that_is_moving_steadily_writes_nothing() {
        let mut riders = vec![rider(0.5, 0.0), rider(0.1, 0.0)];
        for now in 1..4 {
            let outcome = step(&mut riders, RATE, f64::from(now), || false);
            assert!(!outcome.changed, "tick {now}");
        }
    }

    #[test]
    fn changing_the_speed_keeps_everybody_where_they_are() {
        let mut riders = vec![rider(0.0, 0.0), rider(0.0, -5.0)];
        let before = positions(&riders, RATE, 3.0);
        rebase(&mut riders, RATE, 3.0);
        let after = positions(&riders, RATE * 2.0, 3.0);
        for (a, b) in before.iter().zip(&after) {
            assert!((a - b).abs() < 1e-6);
        }
        // And from there they move at the new rate.
        let later = positions(&riders, RATE * 2.0, 4.0);
        assert!((later[0] - (before[0] + RATE * 2.0)).abs() < 1e-5);
    }

    #[test]
    fn turning_the_flow_around_keeps_every_item_in_its_place() {
        let mut riders = vec![rider(0.7, 0.0), rider(0.2, 0.0)];
        flip(&mut riders, 0.0);
        // The one that was nearest the old exit is now nearest the new entrance, so last.
        assert!((riders[0].p - 0.8).abs() < 1e-6);
        assert!((riders[1].p - 0.3).abs() < 1e-6);
        // Front first still holds.
        assert!(riders[0].p > riders[1].p);
    }

    #[test]
    fn a_new_rider_goes_behind_the_others() {
        let mut riders = vec![rider(0.9, 0.0)];
        push(&mut riders, 5.0);
        assert_eq!(riders.len(), 2);
        assert_eq!(riders[1].p, 0.0);
        assert_eq!(riders[1].t0, 5.0);
    }
}

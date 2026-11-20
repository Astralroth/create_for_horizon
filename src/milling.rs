//! The rules of the Millstone with no game types: its stress and whether a result fits in the
//! output slot. The adapter in `millstone.rs` reads and writes the slots.

/// Stress units the millstone uses for every RPM it turns at.
pub const IMPACT: f32 = 4.0;
/// The most items the output slot is allowed to hold. Every item the starter recipes make stacks
/// to 64, and the game checks the real maximum when the slot is written, so a smaller cap here
/// only keeps the millstone from asking for more than it can get.
pub const MAX_STACK: u32 = 64;

/// Whether `add` more items fit on a stack of `existing` without going over `max`.
pub fn can_add(existing: u32, add: u32, max: u32) -> bool {
    existing.checked_add(add).is_some_and(|total| total <= max)
}

/// Whether a result of `count` items fits in an output slot: an empty slot (`None`) takes it if
/// it is no more than a stack, and one that holds `Some((same_kind, existing))` takes it if
/// the items are the same kind and the sum stays within a stack.
pub fn result_fits(output: Option<(bool, u32)>, count: u32) -> bool {
    match output {
        None => count <= MAX_STACK,
        Some((same_kind, existing)) => same_kind && can_add(existing, count, MAX_STACK),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_fit_until_the_stack_is_full() {
        assert!(can_add(0, 64, 64));
        assert!(can_add(61, 3, 64));
        assert!(!can_add(62, 3, 64));
        assert!(!can_add(64, 1, 64));
    }

    #[test]
    fn a_huge_count_does_not_overflow() {
        assert!(!can_add(u32::MAX, 1, 64));
        assert!(!can_add(1, u32::MAX, 64));
    }

    #[test]
    fn an_empty_output_takes_a_result_up_to_a_stack() {
        assert!(result_fits(None, 3));
        assert!(result_fits(None, 64));
        assert!(!result_fits(None, 65));
    }

    #[test]
    fn a_used_output_takes_only_the_same_kind_with_room() {
        assert!(result_fits(Some((true, 10)), 3));
        assert!(!result_fits(Some((false, 10)), 3));
        assert!(!result_fits(Some((true, 63)), 2));
        assert!(result_fits(Some((true, 62)), 2));
    }
}

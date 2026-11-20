//! What the machines that act on the block in front of them do, as pure rules: which block each
//! one works on, how much stress it uses and how long the job takes. The adapter reads the block
//! and does the job.

/// A machine that works on the block it faces. Every one has the orientation and the wrench
/// rules of the Mechanical Drill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Worker {
    /// Breaks any block.
    Drill,
    /// Fells logs.
    Saw,
    /// Reaps ripe crops and replants them.
    Harvester,
    /// Tills dirt into farmland.
    Plough,
}

/// Stress units the drill uses for every RPM it turns at.
pub const DRILL_IMPACT: f32 = 8.0;
/// Stress units the saw uses for every RPM; a saw is hungrier than a drill.
pub const SAW_IMPACT: f32 = 12.0;
/// Stress units the harvester uses for every RPM.
pub const HARVESTER_IMPACT: f32 = 6.0;
/// Stress units the plough uses for every RPM.
pub const PLOUGH_IMPACT: f32 = 6.0;

/// Turns of work, in RPM-ticks, the drill needs for each point of a block's hardness.
pub const DRILL_WORK_PER_HARDNESS: f32 = 600.0;
/// The same for the saw: wood is its trade, so it is quicker than the drill on a log.
pub const SAW_WORK_PER_HARDNESS: f32 = 450.0;
/// Work a ripe crop takes to harvest.
pub const HARVEST_WORK: f32 = 300.0;
/// Work a block of dirt takes to till.
pub const PLOUGH_WORK: f32 = 300.0;

impl Worker {
    /// Stress units this machine uses for every RPM it turns at.
    pub fn impact(self) -> f32 {
        match self {
            Worker::Drill => DRILL_IMPACT,
            Worker::Saw => SAW_IMPACT,
            Worker::Harvester => HARVESTER_IMPACT,
            Worker::Plough => PLOUGH_IMPACT,
        }
    }
}

/// A registry name without its namespace: `minecraft:oak_log` is `oak_log`.
pub fn short_name(id: &str) -> &str {
    id.rsplit_once(':').map_or(id, |(_, name)| name)
}

/// Whether a block is a log, a stem, or the bark-covered wood of one: what a saw fells.
pub fn is_log(id: &str) -> bool {
    let name = short_name(id);
    ["_log", "_stem", "_wood", "_hyphae"]
        .iter()
        .any(|ending| name.ends_with(ending))
}

/// The age at which a crop is ripe, or `None` for a block that is not a crop the harvester
/// knows.
pub fn ripe_age(id: &str) -> Option<u32> {
    match short_name(id) {
        "wheat" | "carrots" | "potatoes" => Some(7),
        "beetroots" | "nether_wart" => Some(3),
        _ => None,
    }
}

/// Whether the crop with this registry name and `age` can be harvested now.
pub fn is_ripe(id: &str, age: u32) -> bool {
    ripe_age(id).is_some_and(|ripe| age >= ripe)
}

/// Whether the plough turns this block into farmland.
pub fn tills_to_farmland(id: &str) -> bool {
    matches!(short_name(id), "dirt" | "grass_block" | "dirt_path")
}

/// The work a saw needs on a log of this hardness; a floor keeps a soft block from going at once.
pub fn saw_work(hardness: f32) -> f32 {
    hardness.max(0.5) * SAW_WORK_PER_HARDNESS
}

/// The work a drill needs on a block of this hardness.
pub fn drill_work(hardness: f32) -> f32 {
    hardness.max(0.5) * DRILL_WORK_PER_HARDNESS
}

/// What a machine's saved progress becomes after one tick at `turning` RPM with a job that
/// needs `needed`: how far it has got, and whether the job is now done (then it starts over).
pub fn advance(done: i32, turning: u32, needed: f32) -> (i32, bool) {
    let total = done as f32 + turning as f32;
    if total >= needed {
        (0, true)
    } else {
        (total as i32, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_log_is_a_log_and_nothing_else_is() {
        for id in [
            "minecraft:oak_log",
            "minecraft:spruce_log",
            "minecraft:crimson_stem",
            "minecraft:stripped_birch_log",
            "minecraft:oak_wood",
            "minecraft:warped_hyphae",
        ] {
            assert!(is_log(id), "{id}");
        }
        for id in [
            "minecraft:oak_planks",
            "minecraft:oak_leaves",
            "minecraft:stone",
            "minecraft:oak_sapling",
            "create:shaft",
            "oak_log_pile",
        ] {
            assert!(!is_log(id), "{id}");
        }
        // A name with no namespace is read the same.
        assert!(is_log("birch_log"));
    }

    #[test]
    fn crops_are_ripe_only_at_their_last_age() {
        assert!(!is_ripe("minecraft:wheat", 6));
        assert!(is_ripe("minecraft:wheat", 7));
        assert!(!is_ripe("minecraft:carrots", 3));
        assert!(is_ripe("minecraft:potatoes", 7));
        // Beetroot and nether wart stop at 3.
        assert!(!is_ripe("minecraft:beetroots", 2));
        assert!(is_ripe("minecraft:beetroots", 3));
        assert!(is_ripe("minecraft:nether_wart", 3));
    }

    #[test]
    fn a_block_that_is_not_a_known_crop_is_never_ripe() {
        assert_eq!(ripe_age("minecraft:oak_sapling"), None);
        assert!(!is_ripe("minecraft:grass_block", 99));
        assert!(!is_ripe("minecraft:pumpkin_stem", 7));
    }

    #[test]
    fn only_loose_soil_can_be_tilled() {
        for id in ["minecraft:dirt", "minecraft:grass_block", "minecraft:dirt_path"] {
            assert!(tills_to_farmland(id), "{id}");
        }
        for id in ["minecraft:farmland", "minecraft:stone", "minecraft:sand", "minecraft:mud"] {
            assert!(!tills_to_farmland(id), "{id}");
        }
    }

    #[test]
    fn short_names_drop_the_namespace() {
        assert_eq!(short_name("minecraft:wheat"), "wheat");
        assert_eq!(short_name("wheat"), "wheat");
        assert_eq!(short_name("create:mechanical_saw"), "mechanical_saw");
    }

    #[test]
    fn each_machine_has_its_own_impact_and_the_saw_is_the_hungriest() {
        assert_eq!(Worker::Drill.impact(), DRILL_IMPACT);
        assert_eq!(Worker::Saw.impact(), SAW_IMPACT);
        assert_eq!(Worker::Harvester.impact(), HARVESTER_IMPACT);
        assert_eq!(Worker::Plough.impact(), PLOUGH_IMPACT);
        for other in [Worker::Drill, Worker::Harvester, Worker::Plough] {
            assert!(Worker::Saw.impact() > other.impact());
        }
    }

    #[test]
    fn a_saw_fells_a_log_faster_than_a_drill_breaks_it() {
        // An oak log has hardness 2.
        assert!(saw_work(2.0) < drill_work(2.0));
        assert_eq!(saw_work(2.0), 900.0);
    }

    #[test]
    fn soft_blocks_still_take_some_work() {
        assert_eq!(drill_work(0.0), 0.5 * DRILL_WORK_PER_HARDNESS);
        assert_eq!(saw_work(0.1), 0.5 * SAW_WORK_PER_HARDNESS);
    }

    #[test]
    fn progress_adds_the_speed_each_tick_and_finishes_at_the_needed_work() {
        // 16 RPM against 100 units of work: six ticks of 16 are 96, the seventh finishes it.
        let mut done = 0;
        let mut ticks = 0;
        loop {
            let (next, finished) = advance(done, 16, 100.0);
            ticks += 1;
            if finished {
                assert_eq!(next, 0);
                break;
            }
            done = next;
        }
        assert_eq!(ticks, 7);
    }

    #[test]
    fn a_stopped_machine_makes_no_progress() {
        assert_eq!(advance(40, 0, 100.0), (40, false));
    }

    #[test]
    fn a_faster_machine_finishes_sooner() {
        let ticks = |rpm: u32| {
            let (mut done, mut ticks) = (0, 0);
            loop {
                let (next, finished) = advance(done, rpm, 900.0);
                ticks += 1;
                if finished {
                    return ticks;
                }
                done = next;
            }
        };
        assert!(ticks(32) < ticks(16));
        assert!(ticks(16) < ticks(8));
    }
}

//! The Mechanical Press: while it turns it works on the first item in the container below it (a
//! depot, a chest, anything that has item slots), and when the work is done it swaps that item
//! for the result of a `create:pressing` recipe. The recipes are data files of a recipe type the
//! mod owns, read through the game's recipe table.
use crate::engine::Pos;
use crate::{pressing, visual, work, KEYS, PROGRESS};
use mod_api::{
    self, BlockEntityTickEvent, BlockEntityTickReply, BlockUpdateFlags, HostTransaction,
    ItemResource, ItemStack, LevelRef, ServerHost, Tag, ITEMS,
};

/// The recipe type the press reads.
pub const RECIPE_TYPE: &str = "create:pressing";

/// What the press would do now: which slot it works on, with what, and for how long.
struct Job {
    slot: usize,
    input: ItemResource,
    result: ItemStack,
    needed: f32,
}

/// The cell below the press, where the item to press is.
fn below(pos: Pos) -> Pos {
    [pos[0], pos[1] - 1, pos[2]]
}

/// The first item in the container below that a recipe takes, with the recipe's result.
fn find_job(host: &mut dyn ServerHost, level: LevelRef, pos: Pos) -> Option<Job> {
    let slots = level.capability(host, &ITEMS, below(pos), Some(mod_api::Direction::Up))?;
    let recipes = host.recipes_of_type(RECIPE_TYPE);
    if recipes.is_empty() {
        return None;
    }
    for slot in 0..slots.size(host) {
        if slots.amount(host, slot) == 0 {
            continue;
        }
        let Some(input) = slots.resource(host, slot) else {
            continue;
        };
        for recipe in recipes.iter() {
            let Some(ingredient) = recipe.json.get("ingredient") else {
                continue;
            };
            let Ok(Some(ingredient)) = host.recipe_ingredient(&ingredient.to_string()) else {
                continue;
            };
            if !ingredient.test(&*host, input.as_stack()) {
                continue;
            }
            let Some(result) = recipe.json.get("result") else {
                continue;
            };
            let Ok(Some(result)) = host.recipe_result(&result.to_string()) else {
                continue;
            };
            let needed = pressing::work_of(recipe.json.get("work").and_then(|work| work.as_f64()));
            return Some(Job {
                slot,
                input,
                result,
                needed,
            });
        }
    }
    None
}

/// Swaps one of the pressed item for the result, in one transaction: if the result does not fit
/// anywhere in the container, nothing changes.
fn finish(host: &mut dyn ServerHost, level: LevelRef, pos: Pos, job: &Job) -> bool {
    let Some(slots) = level.capability(host, &ITEMS, below(pos), Some(mod_api::Direction::Up))
    else {
        return false;
    };
    let wanted = job.result.count();
    let output = ItemResource::of(&job.result);
    host.transaction(|host, tx| {
        if slots.extract(host, tx, job.slot, &job.input, 1) != 1 {
            return Err(());
        }
        let mut put = 0;
        for slot in 0..slots.size(host) {
            if put >= wanted {
                break;
            }
            put += slots.insert(host, tx, slot, &output, wanted - put);
        }
        if put < wanted {
            return Err(());
        }
        Ok(())
    })
    .is_ok()
}

/// One tick of a press: it works while it turns and has something to press, and does the
/// swap once it has done the work that recipe needs.
pub fn tick(host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) -> Option<()> {
    let done = match event.entity.nbt.get(&PROGRESS.into()) {
        Some(Tag::Int(value)) => *value,
        _ => return Some(()),
    };
    let keys = KEYS.get()?;
    let pos = event.pos;
    let level = host.level(&host.dimension())?;
    if level.block(host, pos)? != keys.press {
        return Some(());
    }
    let turning = visual::level_from_name(&level.block_property(host, pos, "speed")?)?;
    let job = if turning > 0 {
        find_job(host, level, pos)
    } else {
        None
    };
    let next = match &job {
        Some(job) => {
            let (next, finished) = work::advance(done, turning, job.needed);
            if finished {
                if finish(host, level, pos, job) {
                    crate::play(host, &level, pos, "minecraft:block.anvil.place", 0.5, 1.4);
                    0
                } else {
                    // The result had nowhere to go: wait with the work done instead of
                    // starting over, and try again next tick.
                    done
                }
            } else {
                next
            }
        }
        // Stopped, or nothing to press: the progress is lost.
        None => 0,
    };
    // The ram only moves while there is a job, so the clients are told when that changes.
    let working = if job.is_some() { "true" } else { "false" };
    if level.block_property(host, pos, "working").as_deref() != Some(working) {
        host.set_block_property_with(pos, "working", working, BlockUpdateFlags::CLIENTS.bits());
    }
    if next != done {
        event.entity.nbt.insert(PROGRESS.into(), Tag::Int(next));
        *event.reply = BlockEntityTickReply::Changed;
    }
    Some(())
}

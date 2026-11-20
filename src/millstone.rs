//! The Millstone: two item slots, an input and an output. While it turns it grinds the item in the
//! input slot into the result of a `create:milling` recipe, one item at a time, and puts it in the
//! output slot. Hoppers fill it from above and the sides and empty it from below; players put items
//! in with a right click and take the output with an empty hand.
use crate::{milling, recipes, visual, work, KEYS, PROGRESS};
use mod_api::{
    self, BlockClick, BlockEntityTickEvent, BlockEntityTickReply, HostTransaction,
    InteractionResult, ModContainer, ServerHost, Tag,
};

/// The recipe type the millstone reads.
pub const RECIPE_TYPE: &str = "create:milling";
/// The slot items go into, and the one the results come out of.
const INPUT: usize = 0;
const OUTPUT: usize = 1;

/// The container the game gives the millstone: hoppers above and beside it fill the input slot,
/// and a hopper below takes from the output slot.
pub fn container() -> ModContainer {
    // The faces are in the order down, up, north, south, west, east.
    let mut faces: [Vec<usize>; 6] = std::array::from_fn(|_| vec![INPUT]);
    faces[0] = vec![OUTPUT];
    ModContainer {
        slots: 2,
        faces,
        inputs: vec![INPUT],
        outputs: vec![OUTPUT],
        menu: None,
        ..Default::default()
    }
}

/// One tick of a millstone: it grinds while it turns and has something to grind and room for
/// the result, and does the swap once it has done the work the recipe needs.
pub fn tick(host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) -> Option<()> {
    let done = match event.entity.nbt.get(&PROGRESS.into()) {
        Some(Tag::Int(value)) => *value,
        _ => return Some(()),
    };
    let keys = KEYS.get()?;
    let pos = event.pos;
    let level = host.level(&host.dimension())?;
    if level.block(host, pos)? != keys.millstone {
        return Some(());
    }
    let turning = visual::level_from_name(&level.block_property(host, pos, "speed")?)?;
    let input = event.entity.items.get(INPUT).cloned().flatten();
    let output = event.entity.items.get(OUTPUT).cloned().flatten();
    let job = if turning > 0 {
        input
            .as_ref()
            .and_then(|stack| recipes::matching(&*host, RECIPE_TYPE, stack))
            .filter(|(result, _)| {
                let held = output
                    .as_ref()
                    .map(|out| (out.is_stackable_with(result), out.count()));
                milling::result_fits(held, result.count())
            })
    } else {
        None
    };
    let mut changed = false;
    let next = match (&job, input) {
        (Some((result, needed)), Some(input)) => {
            let (next, finished) = work::advance(done, turning, *needed);
            if finished {
                let merged = match output {
                    Some(mut out) => out.grow(result.count()).ok().map(|()| out),
                    None => Some(result.clone()),
                };
                if let Some(merged) = merged {
                    event.entity.items[INPUT] = input.shrink(1);
                    event.entity.items[OUTPUT] = Some(merged);
                    changed = true;
                    crate::play(host, &level, pos, "minecraft:block.grindstone.use", 0.4, 0.8);
                    0
                } else {
                    // The result no longer fits: wait with the work done.
                    done
                }
            } else {
                next
            }
        }
        // Stopped, nothing to grind or no room for the result: the progress is lost.
        _ => 0,
    };
    if next != done {
        event.entity.nbt.insert(PROGRESS.into(), Tag::Int(next));
        changed = true;
    }
    if changed {
        *event.reply = BlockEntityTickReply::Changed;
    }
    Some(())
}

/// Right click with an item: put the whole stack in the input slot, when it is empty or holds
/// the same item with room. Sneaking keeps the click for the item.
pub fn put(host: &mut dyn ServerHost, click: &BlockClick<'_>) -> Option<InteractionResult> {
    let keys = KEYS.get()?;
    let stack = click.stack?;
    let player = host.player(click.player)?;
    if player.is_secondary_use_active(host) == Some(true) {
        return None;
    }
    let level = player.level(host)?;
    if level.block(host, click.pos)? != keys.millstone {
        return None;
    }
    let entity = host.block_entity(level, click.pos)?;
    let merged = match entity.item_slot(host, INPUT as u32).ok()? {
        None => stack.clone(),
        Some(mut have) => {
            if !have.is_stackable_with(stack)
                || !milling::can_add(have.count(), stack.count(), milling::MAX_STACK)
            {
                return None;
            }
            have.grow(stack.count()).ok()?;
            have
        }
    };
    let put = host.transaction(|host, tx| {
        entity
            .set_item_slot(host, tx, INPUT as u32, Some(merged))
            .map_err(|_| ())?;
        entity.set_changed(host).map_err(|_| ())?;
        Ok::<(), ()>(())
    });
    if put.is_err() {
        return None;
    }
    if player.is_creative(host) != Some(true) {
        let _ = mod_api::LivingRef::new(player.entity()).set_hand_item(host, click.hand, None);
    }
    crate::play(host, &level, click.pos, "minecraft:block.stone.place", 0.8, 1.0);
    Some(InteractionResult::SuccessServer)
}

/// Right click with an empty hand: take what the millstone has ground.
pub fn take(host: &mut dyn ServerHost, click: &BlockClick<'_>) -> Option<InteractionResult> {
    let keys = KEYS.get()?;
    if click.stack.is_some() {
        return None;
    }
    let player = host.player(click.player)?;
    let level = player.level(host)?;
    if level.block(host, click.pos)? != keys.millstone {
        return None;
    }
    let entity = host.block_entity(level, click.pos)?;
    let held = entity.item_slot(host, OUTPUT as u32).ok()??;
    let taken = host.transaction(|host, tx| {
        entity
            .set_item_slot(host, tx, OUTPUT as u32, None)
            .map_err(|_| ())?;
        entity.set_changed(host).map_err(|_| ())?;
        Ok::<(), ()>(())
    });
    if taken.is_err() {
        return None;
    }
    if player.give_stack(host, held.clone()).is_err() {
        let at = click.pos.map(f64::from);
        let _ = level.spawn_item(host, at[0] + 0.5, at[1] + 1.0, at[2] + 0.5, held, 10);
    }
    crate::play(host, &level, click.pos, "minecraft:entity.item.pickup", 0.5, 1.0);
    Some(InteractionResult::SuccessServer)
}

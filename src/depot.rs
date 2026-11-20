//! The Depot: a low block that holds one item on top. Right click with an item puts one in,
//! right click with an empty hand takes it back, and hoppers fill and empty it through the
//! container the mod declares for it. The item is drawn on the block, so the server sends the
//! stack it holds to the clients, which have no item slots of their own.
use mod_api::{
    self, BlockEntityEvent, BlockEntityPart, BlockEntitySubmitEvent, BlockEntityTickEvent,
    BlockEntityTickReply, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockKey, BlockPart,
    ClientHost, Compound, HostTransaction, InteractionResult, ItemDisplayContext, ItemStack,
    ModContainer, ServerHost, Tag, UseItemOnEvent, UseWithoutItemEvent,
};
use std::sync::OnceLock;

/// The one slot of the depot.
const SLOT: u32 = 0;
/// The saved and synced field holding the stack on show, as an item stack's own data. Empty when
/// nothing is on the depot, so a client always learns when the item is gone.
const SHOWN: &str = "Shown";

static DEPOT: OnceLock<BlockKey> = OnceLock::new();

/// Remembers the depot's block key, which the drawing needs.
pub fn remember(block: BlockKey) {
    let _ = DEPOT.set(block);
}

/// The container the game gives the depot: one slot reached from every face, so hoppers and
/// other containers can fill it and empty it from any side.
pub fn container() -> ModContainer {
    ModContainer {
        slots: 1,
        faces: std::array::from_fn(|_| vec![SLOT as usize]),
        inputs: vec![SLOT as usize],
        outputs: vec![SLOT as usize],
        menu: None,
        ..Default::default()
    }
}

/// How many items a player's hand holds after one went onto the depot: all of them in creative
/// mode, where items are not used up, and one fewer otherwise.
pub fn held_after_putting(count: u32, creative: bool) -> u32 {
    if creative { count } else { count.saturating_sub(1) }
}

/// Writes the stack on show into the depot's saved data and tells the clients, which have no
/// item slots and learn what to draw only from that data. `None` clears it.
fn show(
    host: &mut dyn ServerHost,
    entity: mod_api::BlockEntityRef,
    level: mod_api::LevelRef,
    pos: [i32; 3],
    stack: Option<&ItemStack>,
) {
    let shown = stack
        .and_then(|stack| stack.to_nbt(&*host))
        .unwrap_or_default();
    let mut data = Compound::new();
    data.insert(SHOWN.into(), Tag::Compound(shown));
    let _ = entity.merge_data(host, data);
    let _ = level.send_block_updated(host, pos);
}

/// How many copies of the item to draw for a stack of `count`: one for a single item, two up to
/// sixteen and three beyond, as dropped items pile up, so a stack looks like one.
pub fn copies_shown(count: u32) -> u32 {
    match count {
        0 | 1 => 1,
        2..=16 => 2,
        _ => 3,
    }
}

/// Clicks on the depot.
pub struct DepotBlock;

#[mod_api::horizon_part]
impl BlockPart for DepotBlock {
    /// Right click with an item: put one on the depot when it is empty. Sneaking keeps the click
    /// for the item, so a block can still be placed against the depot.
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let stack = click.stack?;
        let player = host.player(click.player)?;
        if player.is_secondary_use_active(host) == Some(true) {
            return None;
        }
        let level = player.level(host)?;
        let entity = host.block_entity(level, click.pos)?;
        if !matches!(entity.item_slot(host, SLOT), Ok(None)) {
            // Already holding something: the click is for the held item as usual.
            return None;
        }
        let mut one = stack.clone();
        one.set_count(1).ok()?;
        let on_show = one.clone();
        let put = host.transaction(|host, tx| {
            entity
                .set_item_slot(host, tx, SLOT, Some(one))
                .map_err(|_| ())?;
            entity.set_changed(host).map_err(|_| ())?;
            Ok::<(), ()>(())
        });
        if put.is_err() {
            return None;
        }
        show(host, entity, level, click.pos, Some(&on_show));
        let creative = player.is_creative(host) == Some(true);
        let left = held_after_putting(stack.count(), creative);
        if left != stack.count() {
            let mut rest = stack.clone();
            let hand = if left == 0 {
                None
            } else {
                rest.set_count(left).ok().map(|()| rest)
            };
            let _ = mod_api::LivingRef::new(player.entity()).set_hand_item(host, click.hand, hand);
        }
        crate::play(host, &level, click.pos, "minecraft:entity.item_frame.add_item", 0.8, 1.0);
        Some(InteractionResult::SuccessServer)
    }

    /// Right click with an empty hand: take the item back.
    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        if click.stack.is_some() {
            return None;
        }
        let player = host.player(click.player)?;
        let level = player.level(host)?;
        let entity = host.block_entity(level, click.pos)?;
        let held = entity.item_slot(host, SLOT).ok()??;
        let taken = host.transaction(|host, tx| {
            entity.set_item_slot(host, tx, SLOT, None).map_err(|_| ())?;
            entity.set_changed(host).map_err(|_| ())?;
            Ok::<(), ()>(())
        });
        if taken.is_err() {
            return None;
        }
        show(host, entity, level, click.pos, None);
        if player.give_stack(host, held.clone()).is_err() {
            // No room in the inventory: let it drop above the depot instead.
            let at = click.pos.map(f64::from);
            let _ = level.spawn_item(host, at[0] + 0.5, at[1] + 0.5, at[2] + 0.5, held, 10);
        }
        crate::play(host, &level, click.pos, "minecraft:entity.item_frame.remove_item", 0.8, 1.0);
        Some(InteractionResult::SuccessServer)
    }
}

/// The depot's block entity: it keeps the synced copy of its item up to date and draws it.
pub struct DepotEntity;

/// A block entity part also needs its block behaviour; `DepotBlock` has all of it already.
#[mod_api::horizon_part]
impl BlockPart for DepotEntity {}

#[mod_api::horizon_part]
impl BlockEntityPart for DepotEntity {
    /// The depot has no saved data of its own; the item is in its slot.
    fn block_entity(
        &self,
        _: mod_api::LevelSide,
        event: &mut BlockEntityEvent<'_>,
    ) -> Option<Compound> {
        (event.block == "create:depot").then(Compound::new)
    }

    fn block_entity_ticks(&self, _: &dyn mod_api::Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    /// Keeps the synced copy of the item in step with the slot: when they differ, the new copy is
    /// saved and sent to the clients.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        *event.reply = BlockEntityTickReply::Unchanged;
        let shown = match event.entity.items.first() {
            Some(Some(stack)) => stack.to_nbt(&*host).unwrap_or_default(),
            _ => Compound::new(),
        };
        if event.entity.nbt.get(&SHOWN.into()) != Some(&Tag::Compound(shown.clone())) {
            event
                .entity
                .nbt
                .insert(SHOWN.into(), Tag::Compound(shown.clone()));
            let mut update = Compound::new();
            update.insert(SHOWN.into(), Tag::Compound(shown));
            event.entity.set_client_update_fields(update);
            *event.reply = BlockEntityTickReply::Changed;
            // Whatever changed the slot (a hopper, say) did not go through a click: tell the
            // clients here too.
            if let Some(level) = host.level(&host.dimension()) {
                let _ = level.send_block_updated(host, event.pos);
            }
        }
    }

    /// What a client gets when the chunk is sent: the item on show.
    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        let mut tag = Compound::new();
        if let Some(shown) = event.entity.nbt.get(&SHOWN.into()) {
            tag.insert(SHOWN.into(), shown.clone());
        }
        Some(tag)
    }

    /// Draws the depot and, lying on it, the item it holds.
    fn block_entity_submit(&self, host: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let Some(block) = DEPOT.get() else { return };
        let draw = &mut *event.draw;
        draw.push();
        draw.block(*block, &[]);
        draw.pop();
        let Some(Tag::Compound(shown)) = event.entity.nbt.get(&SHOWN.into()) else {
            return;
        };
        if shown.is_empty() {
            return;
        }
        let Some(stack) = ItemStack::from_nbt(host, shown) else {
            return;
        };
        // Lying flat on the depot, one copy for a single item and a small pile for a stack.
        for copy in 0..copies_shown(stack.count()) {
            let step = copy as f32;
            draw.push();
            draw.translate([0.5 + 0.04 * step, 0.28 + 0.07 * step, 0.5 - 0.03 * step]);
            draw.rotate_about([1.0, 0.0, 0.0], 90.0, [0.0; 3]);
            draw.item(&stack, ItemDisplayContext::Ground, None, 0);
            draw.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn putting_an_item_uses_one_up_except_in_creative_mode() {
        assert_eq!(held_after_putting(5, false), 4);
        assert_eq!(held_after_putting(1, false), 0);
        assert_eq!(held_after_putting(5, true), 5);
        assert_eq!(held_after_putting(0, false), 0);
    }

    #[test]
    fn a_stack_is_drawn_as_a_small_pile() {
        assert_eq!(copies_shown(1), 1);
        assert_eq!(copies_shown(2), 2);
        assert_eq!(copies_shown(16), 2);
        assert_eq!(copies_shown(17), 3);
        assert_eq!(copies_shown(64), 3);
        // An empty count never happens for a real stack, but it still draws one.
        assert_eq!(copies_shown(0), 1);
    }

    #[test]
    fn the_container_has_one_slot_reached_from_every_face() {
        let container = container();
        assert_eq!(container.slots, 1);
        assert!(container.faces.iter().all(|face| face == &[0]));
        assert_eq!(container.inputs, [0]);
        assert_eq!(container.outputs, [0]);
        assert_eq!(container.menu, None);
    }
}

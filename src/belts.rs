//! The Belt: a flat piece that carries items along it while it turns. The items are kept in the
//! block entity's saved data as riders (see `belt.rs`); the server moves them and hands them on at
//! the end of the piece, and the clients draw them from the same stored values, so nothing is sent
//! each tick. Hoppers, chutes and funnels put items on through the belt's `ITEMS` capability, and a
//! player puts one on with a right click.
use crate::belt::{self, Rider};
use crate::direction::Direction;
use crate::engine::{Axis, Pos};
use crate::{axis_from, visual, KEYS};
use mod_api::{
    self, BlockClick, BlockEntitySubmitEvent, BlockEntityTickEvent, BlockEntityTickReply,
    BlockEntityUpdateTagEvent, BlockSide, ClientHost, Compound, Host, HostBlockStates,
    HostTransaction, InteractionResult, ItemResource, ItemStack, LevelRef, ResourceHandler, ServerHost, Tag,
    Transaction, ITEMS,
};

/// The saved and synced fields: the riders, the way they are going and the speed they were
/// last written at.
const RIDERS: &str = "Belt";
const FLOW: &str = "Dir";
const SPEED: &str = "Rate";
/// How high above the block's floor the items ride, in blocks.
const ITEM_HEIGHT: f32 = 0.74;
/// The belt surface: its height and where it starts and ends across the belt, in blocks.
const SURFACE_Y: f32 = 0.6885;
const ACROSS: [f32; 2] = [3.0 / 16.0, 13.0 / 16.0];

/// What a belt keeps: its riders, the items they carry (in the same order), which way they go
/// (`1` forwards, `-1` backwards) and the signed speed they were last written at.
#[derive(Clone, Debug)]
pub struct State {
    pub riders: Vec<Rider>,
    pub items: Vec<ItemStack>,
    pub flow: i32,
    pub speed: i32,
}

impl State {
    /// Reads a belt's saved data. A rider whose item cannot be read is left out.
    pub fn read(host: &dyn Host, data: &Compound) -> State {
        let mut riders = Vec::new();
        let mut items = Vec::new();
        if let Some(Tag::List(list)) = data.get(&RIDERS.into()) {
            for tag in list {
                let Tag::Compound(entry) = tag else { continue };
                let Some(Tag::Compound(item)) = entry.get(&"Item".into()) else {
                    continue;
                };
                let Some(stack) = ItemStack::from_nbt(host, item) else {
                    continue;
                };
                let p = match entry.get(&"P".into()) {
                    Some(Tag::Int(milli)) => *milli as f32 / 1000.0,
                    _ => continue,
                };
                let t0 = match entry.get(&"T".into()) {
                    Some(Tag::Long(tick)) => *tick as f64,
                    _ => continue,
                };
                let stopped = matches!(entry.get(&"S".into()), Some(Tag::Byte(1)));
                riders.push(Rider { p, t0, stopped });
                items.push(stack);
            }
        }
        let flow = match data.get(&FLOW.into()) {
            Some(Tag::Int(sign)) if *sign < 0 => -1,
            _ => 1,
        };
        let speed = match data.get(&SPEED.into()) {
            Some(Tag::Int(rpm)) => *rpm,
            _ => 0,
        };
        State {
            riders,
            items,
            flow,
            speed,
        }
    }

    /// The saved data of this state.
    pub fn write(&self, host: &dyn Host) -> Compound {
        let mut list = Vec::new();
        for (rider, stack) in self.riders.iter().zip(&self.items) {
            let Some(item) = stack.to_nbt(host) else {
                continue;
            };
            let mut entry = Compound::new();
            entry.insert("Item".into(), Tag::Compound(item));
            entry.insert("P".into(), Tag::Int((rider.p * 1000.0).round() as i32));
            // The tick is whole on the server, where this is written.
            entry.insert("T".into(), Tag::Long(rider.t0.round() as i64));
            entry.insert("S".into(), Tag::Byte(i8::from(rider.stopped)));
            list.push(Tag::Compound(entry));
        }
        let mut data = Compound::new();
        data.insert(RIDERS.into(), Tag::List(list));
        data.insert(FLOW.into(), Tag::Int(self.flow));
        data.insert(SPEED.into(), Tag::Int(self.speed));
        data
    }

    /// Makes the stored riders agree with the speed the belt turns at now: when the speed or the
    /// direction changed, everybody is rewritten where they are, and turned around if the flow
    /// reversed. True when something changed.
    pub fn follow(&mut self, signed_rpm: i32, now: f64) -> bool {
        let sign = belt::flow_sign(signed_rpm);
        let reversed = sign != 0 && sign != self.flow;
        if signed_rpm == self.speed && !reversed {
            return false;
        }
        if !self.riders.is_empty() {
            belt::rebase(&mut self.riders, belt::rate_for(self.speed), now);
            if reversed {
                belt::flip(&mut self.riders, now);
                self.items.reverse();
            }
        }
        if reversed {
            self.flow = sign;
        }
        self.speed = signed_rpm;
        true
    }
}

/// The signed speed the belt at `pos` turns at, from its state: its speed level and direction.
fn signed_rpm(host: &dyn Host, level: &LevelRef, pos: Pos) -> i32 {
    let turning = level
        .block_property(host, pos, "speed")
        .as_deref()
        .and_then(visual::level_from_name)
        .unwrap_or(0) as i32;
    if level.block_property(host, pos, "reverse").as_deref() == Some("true") {
        -turning
    } else {
        turning
    }
}

/// The direction items go along a belt turning about `axis` when its speed is positive: along the
/// shaft, to the east for an x shaft and to the south for a z one. A line of belts is a line of
/// shafts, so one motor at its end turns them all.
fn forward(axis: Axis) -> Direction {
    match axis {
        Axis::X => Direction::East,
        _ => Direction::South,
    }
}

/// The direction the riders of a belt about `axis` go, for a flow of `1` or `-1`.
fn exit_direction(axis: Axis, flow: i32) -> Direction {
    if flow >= 0 {
        forward(axis)
    } else {
        forward(axis).opposite()
    }
}

/// The game's name for one of this mod's directions.
fn sdk_direction(direction: Direction) -> mod_api::Direction {
    match direction {
        Direction::Down => mod_api::Direction::Down,
        Direction::Up => mod_api::Direction::Up,
        Direction::North => mod_api::Direction::North,
        Direction::South => mod_api::Direction::South,
        Direction::West => mod_api::Direction::West,
        Direction::East => mod_api::Direction::East,
    }
}

fn axis_at(host: &dyn Host, level: &LevelRef, pos: Pos) -> Axis {
    level
        .block_property(host, pos, "axis")
        .as_deref()
        .and_then(axis_from)
        .filter(|axis| *axis != Axis::Y)
        .unwrap_or(Axis::Z)
}

/// Hands one item to what is in front of the belt: another belt, a container. False when nothing
/// there takes it.
fn deliver(host: &mut dyn ServerHost, level: LevelRef, target: Pos, side: Direction, stack: &ItemStack) -> bool {
    let Some(slots) = level.capability(host, &ITEMS, target, Some(sdk_direction(side))) else {
        return false;
    };
    let resource = ItemResource::of(stack);
    host.transaction(|host, tx| {
        for slot in 0..slots.size(host) {
            if slots.insert(host, tx, slot, &resource, 1) >= 1 {
                return Ok(());
            }
        }
        Err(())
    })
    .is_ok()
}

/// One tick of a belt: the riders keep going, and the front one is handed on when it gets to the
/// end. Nothing is written unless something changed.
pub fn tick(host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) -> Option<()> {
    // A belt with nobody on it has nothing to do.
    if !matches!(event.entity.nbt.get(&RIDERS.into()), Some(Tag::List(list)) if !list.is_empty()) {
        return Some(());
    }
    let keys = KEYS.get()?;
    let pos = event.pos;
    let level = host.level(&host.dimension())?;
    if level.block(host, pos)? != keys.belt {
        return Some(());
    }
    let mut state = State::read(&*host, &event.entity.nbt);
    let now = level.game_time(host)? as f64;
    let mut changed = state.follow(signed_rpm(&*host, &level, pos), now);
    let axis = axis_at(&*host, &level, pos);
    let way = exit_direction(axis, state.flow);
    let target = way.step(pos);
    let rate = belt::rate_for(state.speed);
    let State { riders, items, .. } = &mut state;
    let outcome = belt::step(riders, rate, now, || match items.first() {
        Some(stack) => deliver(host, level, target, way.opposite(), stack),
        None => false,
    });
    if outcome.left {
        items.remove(0);
    }
    changed |= outcome.changed;
    if changed {
        let data = state.write(&*host);
        for (key, value) in data {
            event.entity.nbt.insert(key, value);
        }
        *event.reply = BlockEntityTickReply::Changed;
        let _ = level.send_block_updated(host, pos);
    }
    Some(())
}

/// Puts one item on the belt at `pos`, at its entrance, when there is room. The door every
/// way of loading a belt goes through. False when it did not fit.
fn accept(host: &mut dyn ServerHost, level: LevelRef, pos: Pos, stack: &ItemStack) -> Option<bool> {
    let keys = KEYS.get()?;
    if level.block(host, pos)? != keys.belt {
        return Some(false);
    }
    let entity = host.block_entity(level, pos)?;
    let data = entity.data(&*host)?;
    let mut state = State::read(&*host, &data);
    let now = level.game_time(host)? as f64;
    state.follow(signed_rpm(&*host, &level, pos), now);
    if !belt::can_accept(&state.riders, belt::rate_for(state.speed), now) {
        return Some(false);
    }
    let mut one = stack.clone();
    one.set_count(1).ok()?;
    belt::push(&mut state.riders, now);
    state.items.push(one);
    let written = state.write(&*host);
    entity.merge_data(host, written).ok()?;
    let _ = level.send_block_updated(host, pos);
    Some(true)
}

/// The belt's door for hoppers, chutes, funnels and other belts: one place, which takes one item
/// at a time and gives nothing back.
pub struct BeltItems {
    at: BlockSide,
}

impl BeltItems {
    pub fn new(at: BlockSide) -> Self {
        Self { at }
    }
}

impl ResourceHandler<ItemResource> for BeltItems {
    fn size(&self, _: &mut dyn ServerHost) -> usize {
        1
    }
    fn resource(&self, _: &mut dyn ServerHost, _: usize) -> Option<ItemResource> {
        None
    }
    fn amount(&self, _: &mut dyn ServerHost, _: usize) -> u32 {
        0
    }
    fn capacity(&self, _: &mut dyn ServerHost, _: usize, _: &ItemResource) -> u32 {
        1
    }
    fn insert(
        &self,
        host: &mut dyn ServerHost,
        _: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        if index != 0 || amount == 0 {
            return 0;
        }
        u32::from(accept(host, self.at.level, self.at.pos, resource.as_stack()).unwrap_or(false))
    }
    fn extract(
        &self,
        _: &mut dyn ServerHost,
        _: &Transaction,
        _: usize,
        _: &ItemResource,
        _: u32,
    ) -> u32 {
        0
    }
}

/// Right click with an item on a belt: put one on it. Sneaking keeps the click for the item.
pub fn put(host: &mut dyn ServerHost, click: &BlockClick<'_>) -> Option<InteractionResult> {
    let keys = KEYS.get()?;
    let stack = click.stack?;
    let player = host.player(click.player)?;
    if player.is_secondary_use_active(host) == Some(true) {
        return None;
    }
    let level = player.level(host)?;
    if level.block(host, click.pos)? != keys.belt {
        return None;
    }
    if !accept(host, level, click.pos, stack)? {
        return None;
    }
    if player.is_creative(host) != Some(true) {
        let rest = stack.clone().shrink(1);
        let _ = mod_api::LivingRef::new(player.entity()).set_hand_item(host, click.hand, rest);
    }
    crate::play(host, &level, click.pos, "minecraft:block.wool.place", 0.6, 1.2);
    Some(InteractionResult::SuccessServer)
}

/// What a client gets when the chunk is sent or the belt changes: the riders, if the entity keeps
/// any. Other kinetic blocks keep none and send nothing.
pub fn update_tag(event: &BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
    event.entity.nbt.get(&RIDERS.into())?;
    let mut tag = Compound::new();
    for key in [RIDERS, FLOW, SPEED] {
        if let Some(value) = event.entity.nbt.get(&key.into()) {
            tag.insert(key.into(), value.clone());
        }
    }
    Some(tag)
}

/// Draws the surface of the belt scrolling at its speed. The texture is cut where it wraps, so
/// it runs on for ever without a seam.
fn surface(draw: &mut dyn mod_api::BlockEntityDrawContext, axis: Axis, scroll: f32) {
    let offset = scroll.rem_euclid(16.0);
    let boundary = offset / 16.0;
    let pieces = [
        (0.0, boundary, 16.0 - offset, 16.0),
        (boundary, 1.0, 0.0, 16.0 - offset),
    ];
    for (t0, t1, v0, v1) in pieces {
        if t1 - t0 < 1.0e-4 {
            continue;
        }
        let (s0, s1) = (ACROSS[0], ACROSS[1]);
        let y = SURFACE_Y;
        // Corners in a model face's order: top left, bottom left, bottom right, top right.
        let corners = match axis {
            Axis::X => [[t0, y, s0], [t1, y, s0], [t1, y, s1], [t0, y, s1]],
            _ => [[s0, y, t0], [s0, y, t1], [s1, y, t1], [s1, y, t0]],
        };
        let uv = [s0 * 16.0, v0, s1 * 16.0, v1];
        draw.quad("create:block/belt_surface", corners, uv, 0xFFFFFF);
        // The same quad wound the other way, so it shows whichever way the game faces it.
        let flipped = [corners[0], corners[3], corners[2], corners[1]];
        draw.quad("create:block/belt_surface", flipped, uv, 0xFFFFFF);
    }
}

/// Draws a belt: its body, the moving surface and the items riding on it.
pub fn submit(host: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>, block: mod_api::BlockKey) {
    let states = host.block_states();
    let property = |name: &str| event.state.property(&states, name).ok().flatten();
    let Some(axis_name) = property("axis") else {
        return;
    };
    let axis = axis_from(axis_name)
        .filter(|axis| *axis != Axis::Y)
        .unwrap_or(Axis::Z);
    let turning = property("speed")
        .and_then(visual::level_from_name)
        .unwrap_or(0) as i32;
    let signed = if property("reverse") == Some("true") {
        -turning
    } else {
        turning
    };
    let state = State::read(host, &event.entity.nbt);
    let draw = &mut *event.draw;
    draw.push();
    draw.block(block, &[("axis", axis_name)]);
    draw.pop();

    let now = draw.game_time() as f64 + f64::from(draw.partial());
    // The surface moves with the speed, the way the belt turns.
    let rate = belt::rate_for(signed);
    let scroll = (draw.animation_time() as f32 * rate * 16.0) * belt::flow_sign(signed) as f32;
    draw.push();
    surface(draw, axis, scroll);
    draw.pop();

    let places = belt::positions(&state.riders, belt::rate_for(state.speed), now);
    for (stack, progress) in state.items.iter().zip(places) {
        let along = if state.flow >= 0 { progress } else { 1.0 - progress };
        let at = match axis {
            Axis::X => [along, ITEM_HEIGHT, 0.5],
            _ => [0.5, ITEM_HEIGHT, along],
        };
        draw.push();
        draw.translate(at);
        draw.rotate_about([1.0, 0.0, 0.0], 90.0, [0.0; 3]);
        draw.item(stack, mod_api::ItemDisplayContext::Ground, None, 0);
        draw.pop();
    }
}

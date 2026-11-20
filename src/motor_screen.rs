//! The Creative Motor's screen: a slider that sets its speed in both directions, from -256 to 256
//! RPM, and a button for each direction to move it one step. The server keeps the signed speed in
//! the block entity's one data slot, which the screen reads; a press on the screen reaches the
//! server as a menu button and sets the motor's `rpm` and `invert`.
use crate::{motor, KEYS};
use mod_api::{
    self, BlockEntityTickEvent, BlockEntityTickReply, DrawContext, MenuButtonEvent, ModContainer,
    Screen, ScreenClickEvent, ScreenDrawEvent, ServerHost,
};

/// The block that has the screen.
const BLOCK: &str = "create:creative_motor";
/// The menu type declared in `data/create/horizon/menu/motor.json`.
const MENU: &str = "create:motor";

/// The screen's size in GUI pixels; the standard container panel.
const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
/// The slider's track: where it starts, how wide it is and where it sits.
const TRACK_LEFT: f32 = 12.0;
const TRACK_WIDTH: f32 = 152.0;
const TRACK_TOP: f32 = 40.0;
/// The rows a press on the slider counts in, which is taller than the track to be easy to hit.
const HIT_TOP: f32 = 30.0;
const HIT_HEIGHT: f32 = 24.0;
/// The two step buttons: left (towards backwards) and right (towards forwards).
const BUTTON_Y: f32 = 60.0;
const BUTTON_SIZE: [f32; 2] = [20.0, 14.0];
const BUTTON_BACK_X: f32 = TRACK_LEFT;
const BUTTON_FORWARD_X: f32 = TRACK_LEFT + TRACK_WIDTH - BUTTON_SIZE[0];
/// The menu button ids: 0 to 17 pick a slider position, these two step from where it is.
const BUTTON_STEP_BACK: i32 = 100;
const BUTTON_STEP_FORWARD: i32 = 101;

const BACKWARDS: u32 = 0xFF3A6EA5;
const FORWARDS: u32 = 0xFFC9822B;
const KNOB: u32 = 0xFFFFFFFF;
const TICK: u32 = 0xFF1E1E1E;
const BUTTON: u32 = 0xFF555555;
const BUTTON_EDGE: u32 = 0xFF1E1E1E;

/// The container the game gives the motor: one slot nobody can reach (it only exists because a
/// menu has at least one), and the menu that opens when the block is used.
pub fn container() -> ModContainer {
    ModContainer {
        slots: 1,
        faces: std::array::from_fn(|_| Vec::new()),
        inputs: Vec::new(),
        outputs: Vec::new(),
        menu: Some(MENU.to_string()),
        ..Default::default()
    }
}

/// How the motor's screen is laid out: the standard panel with its one slot hidden.
pub fn layout(block: &str) -> Option<Screen> {
    (block == BLOCK).then(|| Screen {
        width: WIDTH,
        height: HEIGHT,
        slots: vec![None],
        inventory_y: 84.0,
        inventory_x: 8.0,
        ..Default::default()
    })
}

/// Keeps the menu's data slot at the motor's signed speed, so the screen shows it.
pub fn sync(host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) -> Option<()> {
    let keys = KEYS.get()?;
    let level = host.level(&host.dimension())?;
    if level.block(host, event.pos)? != keys.motor {
        return Some(());
    }
    let (rpm, invert) = motor::read(|name| level.block_property(host, event.pos, name));
    let value = motor::signed_int(rpm, invert);
    if event.entity.data.first() != Some(&value) {
        event.entity.data = vec![value];
        *event.reply = BlockEntityTickReply::Changed;
    }
    Some(())
}

/// Draws a rectangle given as left, top, width, height relative to the screen's corner.
fn rect(draw: &mut dyn DrawContext, origin: [f32; 2], x: f32, y: f32, w: f32, h: f32, argb: u32) {
    draw.fill([origin[0] + x, origin[1] + y, w, h], argb);
}

/// Draws the screen: the speed, the slider with its knob, and the two step buttons.
pub fn draw(event: &mut ScreenDrawEvent<'_>) {
    if event.block != BLOCK {
        return;
    }
    beat();
    let origin = event.origin;
    let signed = event.data.first().copied().unwrap_or(motor::DEFAULT_RPM as i32);
    let knob_at = motor::slider_index(signed);
    let draw = &mut *event.draw;

    let label = format!("{signed} RPM");
    let label_x = (WIDTH - draw.text_width(&label)) / 2.0;
    draw.text(&label, [origin[0] + label_x, origin[1] + 20.0], 0xFF404040, false);

    // The track: blue behind the centre, orange ahead of it, with a tick at every position and a
    // taller one in the middle where the direction changes.
    let middle = (motor::slider_x(8, TRACK_LEFT, TRACK_WIDTH) + motor::slider_x(9, TRACK_LEFT, TRACK_WIDTH)) / 2.0;
    rect(draw, origin, TRACK_LEFT, TRACK_TOP, middle - TRACK_LEFT, 4.0, BACKWARDS);
    rect(draw, origin, middle, TRACK_TOP, TRACK_LEFT + TRACK_WIDTH - middle, 4.0, FORWARDS);
    for index in 0..motor::SLIDER_STEPS {
        let x = motor::slider_x(index, TRACK_LEFT, TRACK_WIDTH);
        rect(draw, origin, x - 0.5, TRACK_TOP - 3.0, 1.0, 10.0, TICK);
    }
    rect(draw, origin, middle - 0.5, TRACK_TOP - 6.0, 1.0, 16.0, TICK);
    let knob_x = motor::slider_x(knob_at, TRACK_LEFT, TRACK_WIDTH);
    rect(draw, origin, knob_x - 3.0, TRACK_TOP - 7.0, 6.0, 18.0, KNOB);
    rect(draw, origin, knob_x - 2.0, TRACK_TOP - 6.0, 4.0, 16.0, if signed < 0 { BACKWARDS } else { FORWARDS });

    draw.text("-256", [origin[0] + TRACK_LEFT - 4.0, origin[1] + 50.0], 0xFF404040, false);
    let right = "256";
    let right_x = TRACK_LEFT + TRACK_WIDTH - draw.text_width(right) + 4.0;
    draw.text(right, [origin[0] + right_x, origin[1] + 50.0], 0xFF404040, false);

    // The step buttons.
    for (x, symbol) in [(BUTTON_BACK_X, "<"), (BUTTON_FORWARD_X, ">")] {
        rect(draw, origin, x, BUTTON_Y, BUTTON_SIZE[0], BUTTON_SIZE[1], BUTTON_EDGE);
        rect(draw, origin, x + 1.0, BUTTON_Y + 1.0, BUTTON_SIZE[0] - 2.0, BUTTON_SIZE[1] - 2.0, BUTTON);
        let symbol_x = x + (BUTTON_SIZE[0] - draw.text_width(symbol)) / 2.0;
        draw.text(symbol, [origin[0] + symbol_x, origin[1] + BUTTON_Y + 3.0], 0xFFFFFFFF, false);
    }
}

/// Whether a point (relative to the screen's corner) is inside a rectangle.
fn inside(at: [f32; 2], x: f32, y: f32, w: f32, h: f32) -> bool {
    at[0] >= x && at[0] < x + w && at[1] >= y && at[1] < y + h
}

/// What a press on the screen means: a slider position (0 to 17) or a step button, as the menu
/// button id the server is sent. `None` for a press anywhere else.
pub fn press_at(at: [f32; 2]) -> Option<i32> {
    if inside(at, BUTTON_BACK_X, BUTTON_Y, BUTTON_SIZE[0], BUTTON_SIZE[1]) {
        return Some(BUTTON_STEP_BACK);
    }
    if inside(at, BUTTON_FORWARD_X, BUTTON_Y, BUTTON_SIZE[0], BUTTON_SIZE[1]) {
        return Some(BUTTON_STEP_FORWARD);
    }
    if at[1] >= HIT_TOP && at[1] < HIT_TOP + HIT_HEIGHT {
        return motor::index_at(at[0], TRACK_LEFT, TRACK_WIDTH).map(|index| index as i32);
    }
    None
}

/// The signed speed a pointer at `at` (relative to the screen's corner) is over on the slider, for
/// the tooltip; `None` anywhere that is not the track.
pub fn speed_under(at: [f32; 2]) -> Option<i32> {
    match press_at(at)? {
        index @ 0..=17 => motor::slider_values().get(index as usize).copied(),
        _ => None,
    }
}

/// The tooltip over the motor's screen: the speed under the pointer, when it is on the slider.
pub fn tooltip(block: &str, at: [f32; 2], lines: &mut Vec<String>) {
    if block != BLOCK {
        return;
    }
    if let Some(speed) = speed_under(at) {
        lines.push(format!("{speed} RPM"));
    }
}

/// The menu button an arrow key stands for: left steps backwards, right steps forwards.
pub fn key_button(key: &str) -> Option<i32> {
    match key {
        "key.keyboard.left" => Some(BUTTON_STEP_BACK),
        "key.keyboard.right" => Some(BUTTON_STEP_FORWARD),
        _ => None,
    }
}

/// How long after the last frame of the motor's screen it still counts as open, in milliseconds.
const OPEN_FOR_MS: u64 = 250;

/// Whether a screen drawn at `last_drawn` is still open at `now` (both in milliseconds). Nothing
/// tells a key hook which screen is open, so the screen's own drawing, once a frame, says so.
pub fn is_open(last_drawn: Option<u64>, now: u64) -> bool {
    last_drawn.is_some_and(|last| now >= last && now - last <= OPEN_FOR_MS)
}

/// Milliseconds since the first call, the clock the screen's heartbeat uses.
fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// The time the motor's screen was last drawn, plus one so zero means never.
static LAST_DRAWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Notes that the motor's screen was drawn just now.
fn beat() {
    LAST_DRAWN.store(now_ms() + 1, std::sync::atomic::Ordering::Relaxed);
}

/// An arrow key while the motor's screen is open steps the speed.
pub fn key(host: &mut dyn mod_api::ClientHost, event: &mod_api::InputKeyEvent<'_>) {
    if !matches!(
        event.action,
        mod_api::KeyAction::Press | mod_api::KeyAction::Repeat
    ) {
        return;
    }
    let Some(button) = key_button(event.key) else {
        return;
    };
    let last = LAST_DRAWN.load(std::sync::atomic::Ordering::Relaxed);
    if !is_open(last.checked_sub(1), now_ms()) {
        return;
    }
    let Some(player) = host.local_player() else {
        return;
    };
    if let Some(menu) = player.container_menu(&*host) {
        menu.click_button(host, button);
    }
}

/// A press on the screen: only the left button, only the motor's own screen.
pub fn click(event: &ScreenClickEvent<'_>) -> Option<i32> {
    if event.block != BLOCK || event.right {
        return None;
    }
    press_at(event.at)
}

/// The signed speed a menu button asks for, given the speed now.
pub fn target(button: i32, current: i32) -> Option<i32> {
    match button {
        0..=17 => motor::slider_values().get(button as usize).copied(),
        BUTTON_STEP_BACK => Some(motor::step_by(current, -1)),
        BUTTON_STEP_FORWARD => Some(motor::step_by(current, 1)),
        _ => None,
    }
}

/// A menu button pressed on the server: set the motor's speed and direction.
pub fn press(host: &mut dyn ServerHost, event: &mut MenuButtonEvent<'_>) {
    let Some(keys) = KEYS.get() else { return };
    let pos = event.context.pos;
    let Some(level) = host.level(&host.dimension()) else {
        return;
    };
    if level.block(host, pos) != Some(keys.motor) {
        return;
    }
    let (rpm, invert) = motor::read(|name| level.block_property(host, pos, name));
    let Some(signed) = target(event.context.id, motor::signed_int(rpm, invert)) else {
        return;
    };
    let (rpm, invert) = motor::split(signed);
    host.set_block_property(pos, "rpm", motor::name(rpm));
    host.set_block_property(pos, "invert", if invert { "true" } else { "false" });
    crate::play(host, &level, pos, "minecraft:block.lever.click", 0.8, motor::click_pitch(rpm));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_on_the_track_picks_a_position_and_one_on_a_button_steps() {
        let y = TRACK_TOP + 2.0;
        assert_eq!(press_at([TRACK_LEFT, y]), Some(0));
        assert_eq!(press_at([TRACK_LEFT + TRACK_WIDTH, y]), Some(17));
        let middle = motor::slider_x(8, TRACK_LEFT, TRACK_WIDTH) + 1.0;
        assert_eq!(press_at([middle, y]), Some(8));
        let button_y = BUTTON_Y + 2.0;
        assert_eq!(press_at([BUTTON_BACK_X + 2.0, button_y]), Some(BUTTON_STEP_BACK));
        assert_eq!(press_at([BUTTON_FORWARD_X + 2.0, button_y]), Some(BUTTON_STEP_FORWARD));
    }

    #[test]
    fn a_press_anywhere_else_does_nothing() {
        assert_eq!(press_at([5.0, 5.0]), None);
        assert_eq!(press_at([TRACK_LEFT, HIT_TOP - 1.0]), None);
        assert_eq!(press_at([TRACK_LEFT, HIT_TOP + HIT_HEIGHT]), None);
        // Between the two buttons, below the track.
        assert_eq!(press_at([WIDTH / 2.0, BUTTON_Y + 2.0]), None);
        assert_eq!(press_at([300.0, TRACK_TOP]), None);
    }

    #[test]
    fn the_tooltip_names_the_speed_under_the_pointer_on_the_slider_only() {
        let y = TRACK_TOP + 2.0;
        assert_eq!(speed_under([TRACK_LEFT, y]), Some(-256));
        assert_eq!(speed_under([TRACK_LEFT + TRACK_WIDTH, y]), Some(256));
        assert_eq!(speed_under([5.0, 5.0]), None);
        // The step buttons are not the slider.
        assert_eq!(speed_under([BUTTON_BACK_X + 2.0, BUTTON_Y + 2.0]), None);
        let mut lines = Vec::new();
        tooltip(BLOCK, [TRACK_LEFT, y], &mut lines);
        assert_eq!(lines, ["-256 RPM"]);
        // Another block's screen, or nothing under the pointer, adds nothing.
        let mut other = Vec::new();
        tooltip("create:shaft", [TRACK_LEFT, y], &mut other);
        tooltip(BLOCK, [5.0, 5.0], &mut other);
        assert!(other.is_empty());
    }

    #[test]
    fn the_arrow_keys_step_the_speed_and_other_keys_do_nothing() {
        assert_eq!(key_button("key.keyboard.left"), Some(BUTTON_STEP_BACK));
        assert_eq!(key_button("key.keyboard.right"), Some(BUTTON_STEP_FORWARD));
        assert_eq!(key_button("key.keyboard.a"), None);
        assert_eq!(key_button("key.keyboard.up"), None);
        assert_eq!(key_button(""), None);
    }

    #[test]
    fn a_screen_counts_as_open_for_a_moment_after_its_last_frame() {
        assert!(is_open(Some(1000), 1000));
        assert!(is_open(Some(1000), 1000 + OPEN_FOR_MS));
        assert!(!is_open(Some(1000), 1000 + OPEN_FOR_MS + 1));
        // Never drawn, or a clock that went backwards: closed.
        assert!(!is_open(None, 5000));
        assert!(!is_open(Some(2000), 1000));
    }

    #[test]
    fn the_buttons_do_not_overlap_the_track_hit_area() {
        assert!(BUTTON_Y >= HIT_TOP + HIT_HEIGHT);
    }

    #[test]
    fn every_slider_button_asks_for_its_own_speed() {
        for (index, value) in motor::slider_values().into_iter().enumerate() {
            assert_eq!(target(index as i32, 16), Some(value));
        }
        assert_eq!(target(18, 16), None);
        assert_eq!(target(-1, 16), None);
    }

    #[test]
    fn the_step_buttons_move_one_position_from_where_the_motor_is() {
        assert_eq!(target(BUTTON_STEP_FORWARD, 16), Some(32));
        assert_eq!(target(BUTTON_STEP_BACK, 16), Some(8));
        assert_eq!(target(BUTTON_STEP_BACK, 1), Some(-1));
        assert_eq!(target(BUTTON_STEP_FORWARD, 256), Some(256));
    }

    #[test]
    fn the_layout_is_only_for_the_motor_and_hides_its_single_slot() {
        let screen = layout(BLOCK).expect("the motor has a screen");
        assert_eq!(screen.slots, vec![None]);
        assert_eq!(screen.width, WIDTH);
        assert!(layout("create:shaft").is_none());
    }

    #[test]
    fn the_container_has_one_unreachable_slot_and_names_the_menu() {
        let container = container();
        assert_eq!(container.slots, 1);
        assert!(container.faces.iter().all(|face| face.is_empty()));
        assert!(container.inputs.is_empty() && container.outputs.is_empty());
        assert_eq!(container.menu.as_deref(), Some(MENU));
    }
}

//! The messages the mod shows to players: pure logic that picks a translation key and its
//! arguments. The adapter turns a [`Message`] into game text, so every line can be translated.

use crate::engine::{Network, Pos, Status};

pub const REPORT_IDLE: &str = "create.report.idle";
pub const REPORT_RUNNING: &str = "create.report.running";
pub const REPORT_LOCKED: &str = "create.report.locked";
pub const REPORT_OVERSTRESSED: &str = "create.report.overstressed";
pub const GAUGE_EMPTY: &str = "create.report.gauge_empty";
pub const MOTOR_SPEED: &str = "create.motor.speed";
pub const MOTOR_SPEED_REVERSED: &str = "create.motor.speed_reversed";
pub const MOTOR_REVERSED: &str = "create.motor.reversed";
pub const MOTOR_NORMAL: &str = "create.motor.normal";
pub const LOADED: &str = "create.loaded";

/// Every translation key the messages use. A test checks each one against the language files.
#[cfg(test)]
pub const KEYS: [&str; 10] = [
    REPORT_IDLE,
    REPORT_RUNNING,
    REPORT_LOCKED,
    REPORT_OVERSTRESSED,
    GAUGE_EMPTY,
    MOTOR_SPEED,
    MOTOR_SPEED_REVERSED,
    MOTOR_REVERSED,
    MOTOR_NORMAL,
    LOADED,
];

/// A translation key with the values that fill its `%1$s`, `%2$s`... slots.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub key: &'static str,
    pub arguments: Vec<String>,
}

impl Message {
    fn new(key: &'static str, arguments: Vec<String>) -> Self {
        Self { key, arguments }
    }

    /// A message with nothing to fill in.
    pub fn plain(key: &'static str) -> Self {
        Self::new(key, Vec::new())
    }
}

/// What a click on the block at `pos` reports about its network.
pub fn report(network: &Network, pos: Pos) -> Message {
    let blocks = network.members.len().to_string();
    let rpm = network.member(pos).map_or(0.0, |member| member.rpm);
    match network.status {
        Status::Idle => Message::new(REPORT_IDLE, vec![blocks]),
        Status::Running => Message::new(
            REPORT_RUNNING,
            vec![
                rpm.abs().to_string(),
                blocks,
                format!("{:.0}", network.stress),
                format!("{:.0}", network.capacity),
            ],
        ),
        Status::Conflict => Message::new(REPORT_LOCKED, vec![blocks]),
        Status::Overstressed => Message::new(
            REPORT_OVERSTRESSED,
            vec![
                format!("{:.0}", network.stress),
                format!("{:.0}", network.capacity),
            ],
        ),
    }
}

/// What the motor says after its speed was stepped to `rpm`.
pub fn motor_speed(rpm: u32, inverted: bool) -> Message {
    let key = if inverted { MOTOR_SPEED_REVERSED } else { MOTOR_SPEED };
    Message::new(key, vec![rpm.to_string()])
}

/// What the motor says after the wrench flipped its direction; `inverted` is the new state.
pub fn motor_direction(inverted: bool) -> Message {
    Message::plain(if inverted { MOTOR_REVERSED } else { MOTOR_NORMAL })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Axis, Kind, solve};

    fn line(kinds: &[Kind]) -> Network {
        let nodes: Vec<(Pos, Kind)> = kinds
            .iter()
            .enumerate()
            .map(|(x, kind)| ([x as i32, 0, 0], *kind))
            .collect();
        solve(
            |pos| nodes.iter().find(|(at, _)| *at == pos).map(|(_, kind)| *kind),
            [0, 0, 0],
        )
        .unwrap()
    }

    fn shaft() -> Kind {
        Kind::Shaft { axis: Axis::X }
    }

    fn motor(rpm: f32) -> Kind {
        Kind::Generator {
            axis: Axis::X,
            rpm,
            capacity: 1024.0,
        }
    }

    #[test]
    fn an_idle_network_says_how_many_blocks_are_connected() {
        // Two shafts in a line are joined, but with no source nothing turns.
        let message = report(&line(&[shaft(), shaft()]), [0, 0, 0]);
        assert_eq!(message.key, REPORT_IDLE);
        assert_eq!(message.arguments, ["2"]);
        let alone = report(&line(&[shaft()]), [0, 0, 0]);
        assert_eq!(alone.arguments, ["1"]);
    }

    #[test]
    fn a_running_network_gives_speed_size_and_stress() {
        let network = line(&[
            motor(16.0),
            Kind::Consumer {
                axis: Axis::X,
                impact: 8.0,
            },
        ]);
        let message = report(&network, [1, 0, 0]);
        assert_eq!(message.key, REPORT_RUNNING);
        // 16 RPM, two blocks, 16 x 8 = 128 stress units of the motor's 16 x 1024 = 16384.
        assert_eq!(message.arguments, ["16", "2", "128", "16384"]);
    }

    #[test]
    fn the_speed_has_no_sign_and_is_the_clicked_blocks_own() {
        let network = line(&[motor(-8.0), shaft()]);
        let message = report(&network, [1, 0, 0]);
        assert_eq!(message.arguments[0], "8");
        assert!(message.arguments.iter().all(|argument| !argument.contains('-')));
    }

    #[test]
    fn a_locked_and_an_overstressed_network_have_their_own_messages() {
        let locked = line(&[motor(16.0), shaft(), motor(-16.0)]);
        assert_eq!(report(&locked, [0, 0, 0]).key, REPORT_LOCKED);

        let overstressed = line(&[
            motor(1.0),
            Kind::Consumer {
                axis: Axis::X,
                impact: 5000.0,
            },
        ]);
        let message = report(&overstressed, [0, 0, 0]);
        assert_eq!(message.key, REPORT_OVERSTRESSED);
        assert_eq!(message.arguments, ["5000", "1024"]);
    }

    #[test]
    fn the_motor_messages_name_the_speed_and_the_direction() {
        assert_eq!(motor_speed(32, false).key, MOTOR_SPEED);
        assert_eq!(motor_speed(32, true).key, MOTOR_SPEED_REVERSED);
        assert_eq!(motor_speed(32, true).arguments, ["32"]);
        assert_eq!(motor_direction(true).key, MOTOR_REVERSED);
        assert_eq!(motor_direction(false).key, MOTOR_NORMAL);
        assert!(motor_direction(true).arguments.is_empty());
    }

    /// The keys a language file defines, read from its `"key": "value"` lines.
    fn keys_of(text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix('"')?;
                let (key, after) = rest.split_once('"')?;
                after.trim_start().starts_with(':').then(|| key.to_string())
            })
            .collect()
    }

    const ENGLISH: &str = include_str!("../assets/create/lang/en_us.json");
    const SPANISH: &str = include_str!("../assets/create/lang/es_es.json");

    #[test]
    fn every_message_key_is_translated_in_both_languages() {
        for (name, text) in [("en_us", ENGLISH), ("es_es", SPANISH)] {
            let keys = keys_of(text);
            for key in KEYS {
                assert!(keys.iter().any(|k| k == key), "{name} lacks {key}");
            }
        }
    }

    #[test]
    fn the_two_languages_define_the_same_keys() {
        let mut english = keys_of(ENGLISH);
        let mut spanish = keys_of(SPANISH);
        english.sort();
        spanish.sort();
        assert_eq!(english, spanish);
        assert!(english.len() > 15);
    }

    #[test]
    fn messages_take_as_many_arguments_as_their_text_has_slots() {
        // A slot is written %1$s, %2$s... in the language files.
        let slots = |text: &str, key: &str| -> usize {
            let line = text
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("\"{key}\"")))
                .unwrap_or_else(|| panic!("no line for {key}"));
            (1..=9).filter(|n| line.contains(&format!("%{n}$s"))).count()
        };
        let expected = [
            (REPORT_IDLE, 1),
            (REPORT_RUNNING, 4),
            (REPORT_LOCKED, 1),
            (REPORT_OVERSTRESSED, 2),
            (MOTOR_SPEED, 1),
            (MOTOR_SPEED_REVERSED, 1),
            (GAUGE_EMPTY, 0),
            (MOTOR_REVERSED, 0),
            (MOTOR_NORMAL, 0),
            (LOADED, 0),
        ];
        for (key, count) in expected {
            assert_eq!(slots(ENGLISH, key), count, "en_us {key}");
            assert_eq!(slots(SPANISH, key), count, "es_es {key}");
        }
    }
}

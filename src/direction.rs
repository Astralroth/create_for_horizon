//! The six block faces as directions: pure logic with no game types.
//!
//! North is -Z, south +Z, west -X, east +X, down -Y and up +Y, as in the game.

use crate::engine::{Axis, Pos};

/// One of the six directions a block can face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub const ALL: [Direction; 6] = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    /// The state value that spells this direction.
    pub fn name(self) -> &'static str {
        match self {
            Direction::Down => "down",
            Direction::Up => "up",
            Direction::North => "north",
            Direction::South => "south",
            Direction::West => "west",
            Direction::East => "east",
        }
    }

    /// The direction a state value spells.
    pub fn from_name(name: &str) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|direction| direction.name() == name)
    }

    /// The unit step toward this direction.
    pub fn vector(self) -> [i32; 3] {
        match self {
            Direction::Down => [0, -1, 0],
            Direction::Up => [0, 1, 0],
            Direction::North => [0, 0, -1],
            Direction::South => [0, 0, 1],
            Direction::West => [-1, 0, 0],
            Direction::East => [1, 0, 0],
        }
    }

    /// The axis this direction lies on.
    pub fn axis(self) -> Axis {
        match self {
            Direction::Down | Direction::Up => Axis::Y,
            Direction::North | Direction::South => Axis::Z,
            Direction::West | Direction::East => Axis::X,
        }
    }

    /// The position one step from `pos` toward this direction.
    pub fn step(self, pos: Pos) -> Pos {
        let [x, y, z] = self.vector();
        [pos[0] + x, pos[1] + y, pos[2] + z]
    }

    /// The direction on the other side.
    pub fn opposite(self) -> Direction {
        match self {
            Direction::Down => Direction::Up,
            Direction::Up => Direction::Down,
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
        }
    }

    /// The direction a player looks toward, from the game's yaw and pitch in degrees: yaw 0
    /// looks south, 90 west, and a positive pitch looks down. The axis with the largest
    /// component wins; ties go to the vertical axis, then X, then Z.
    pub fn from_look(yaw: f32, pitch: f32) -> Direction {
        let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
        let x = -yaw.sin() * pitch.cos();
        let y = -pitch.sin();
        let z = yaw.cos() * pitch.cos();
        let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
        if ay >= ax && ay >= az {
            if y < 0.0 { Direction::Down } else { Direction::Up }
        } else if ax >= az {
            if x < 0.0 { Direction::West } else { Direction::East }
        } else if z < 0.0 {
            Direction::North
        } else {
            Direction::South
        }
    }

    /// Where this direction ends up after a quarter turn about `about`, the axis of the face a
    /// wrench was used on, by the right-hand rule. A direction on that axis cannot turn about
    /// it, so it flips to the other side.
    pub fn turned_about(self, about: Axis) -> Direction {
        let [x, y, z] = self.vector();
        // The cross product of the axis' unit vector with this direction.
        let turned = match about {
            Axis::X => [0, -z, y],
            Axis::Y => [z, 0, -x],
            Axis::Z => [-y, x, 0],
        };
        Direction::ALL
            .into_iter()
            .find(|direction| direction.vector() == turned)
            .unwrap_or_else(|| self.opposite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_vectors_round_trip() {
        for direction in Direction::ALL {
            assert_eq!(Direction::from_name(direction.name()), Some(direction));
            assert_eq!(
                Direction::ALL
                    .into_iter()
                    .filter(|other| other.vector() == direction.vector())
                    .count(),
                1
            );
        }
        assert_eq!(Direction::from_name("diagonal"), None);
    }

    #[test]
    fn axes_and_opposites_agree() {
        for direction in Direction::ALL {
            assert_eq!(direction.opposite().opposite(), direction);
            assert_eq!(direction.opposite().axis(), direction.axis());
            let [x, y, z] = direction.vector();
            let [ox, oy, oz] = direction.opposite().vector();
            assert_eq!([x + ox, y + oy, z + oz], [0, 0, 0]);
        }
        assert_eq!(Direction::North.axis(), Axis::Z);
        assert_eq!(Direction::East.axis(), Axis::X);
        assert_eq!(Direction::Up.axis(), Axis::Y);
    }

    #[test]
    fn a_step_moves_one_block() {
        assert_eq!(Direction::North.step([5, 64, 5]), [5, 64, 4]);
        assert_eq!(Direction::East.step([5, 64, 5]), [6, 64, 5]);
        assert_eq!(Direction::Down.step([5, 64, 5]), [5, 63, 5]);
        assert_eq!(Direction::Down.opposite().step([5, 63, 5]), [5, 64, 5]);
    }

    #[test]
    fn looks_map_to_the_game_directions() {
        assert_eq!(Direction::from_look(0.0, 0.0), Direction::South);
        assert_eq!(Direction::from_look(90.0, 0.0), Direction::West);
        assert_eq!(Direction::from_look(180.0, 0.0), Direction::North);
        assert_eq!(Direction::from_look(-90.0, 0.0), Direction::East);
        assert_eq!(Direction::from_look(270.0, 0.0), Direction::East);
        assert_eq!(Direction::from_look(0.0, 90.0), Direction::Down);
        assert_eq!(Direction::from_look(0.0, -90.0), Direction::Up);
        // Looking mostly down wins over the horizontal part.
        assert_eq!(Direction::from_look(0.0, 60.0), Direction::Down);
        assert_eq!(Direction::from_look(0.0, 20.0), Direction::South);
    }

    #[test]
    fn a_quarter_turn_follows_the_right_hand_rule() {
        // Counterclockwise seen from above: east -> north -> west -> south -> east.
        assert_eq!(Direction::East.turned_about(Axis::Y), Direction::North);
        assert_eq!(Direction::North.turned_about(Axis::Y), Direction::West);
        assert_eq!(Direction::West.turned_about(Axis::Y), Direction::South);
        assert_eq!(Direction::South.turned_about(Axis::Y), Direction::East);
        // About east, up turns toward south: z = -y... checked through the full cycle.
        let mut direction = Direction::Up;
        let mut seen = Vec::new();
        for _ in 0..4 {
            direction = direction.turned_about(Axis::X);
            seen.push(direction);
        }
        assert_eq!(seen.last(), Some(&Direction::Up));
        assert!(seen.iter().all(|d| d.axis() != Axis::X));
    }

    #[test]
    fn a_direction_on_the_turning_axis_flips() {
        assert_eq!(Direction::Up.turned_about(Axis::Y), Direction::Down);
        assert_eq!(Direction::East.turned_about(Axis::X), Direction::West);
        assert_eq!(Direction::North.turned_about(Axis::Z), Direction::South);
    }

    #[test]
    fn every_turn_changes_the_direction() {
        for direction in Direction::ALL {
            for axis in Axis::ALL {
                assert_ne!(direction.turned_about(axis), direction);
            }
        }
    }
}

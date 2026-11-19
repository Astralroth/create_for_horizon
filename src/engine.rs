//! The kinetic network solver: pure logic with no game types, so it can be tested alone.
//!
//! A kinetic network is a set of connected rotating blocks. The game adapter describes the
//! world through a lookup from a position to the [`Node`] standing there; [`solve`] walks the
//! connected component around a position and says how fast it turns and whether its sources
//! agree.
//!
//! Rules of this first version:
//! * A node connects to a neighbour only through a *port*: a face on which it has a shaft end.
//! * Both nodes must have a port toward each other, so a shaft never joins a block it points
//!   away from.
//! * Shafts have a port at both ends of their axis. A motor has one port, on its output face.
//! * Rotation is signed relative to the positive direction of the shared axis, and speed does
//!   not change along a connection.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A block position.
pub type Pos = [i32; 3];

/// A spatial axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// Index into a [`Pos`].
    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }
}

/// One of the six block faces, as a unit step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Face(pub Pos);

impl Face {
    pub const ALL: [Face; 6] = [
        Face([-1, 0, 0]),
        Face([1, 0, 0]),
        Face([0, -1, 0]),
        Face([0, 1, 0]),
        Face([0, 0, -1]),
        Face([0, 0, 1]),
    ];

    /// The face on the other side.
    pub fn opposite(self) -> Face {
        Face([-self.0[0], -self.0[1], -self.0[2]])
    }

    /// The axis this face lies on.
    pub fn axis(self) -> Axis {
        if self.0[0] != 0 {
            Axis::X
        } else if self.0[1] != 0 {
            Axis::Y
        } else {
            Axis::Z
        }
    }

    /// The position one step through this face.
    pub fn step(self, from: Pos) -> Pos {
        [from[0] + self.0[0], from[1] + self.0[1], from[2] + self.0[2]]
    }
}

/// What a block does in a network.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A straight shaft with a port at both ends of `axis`.
    Shaft { axis: Axis },
    /// A source. `output` is the face its shaft end points out of; `rpm` is signed relative to
    /// the positive direction of that face's axis.
    Motor { output: Face, rpm: f32 },
    /// A source with a shaft end at both ends of `axis`, like a shaft that also drives.
    /// `rpm` is signed relative to the positive direction of `axis`.
    Generator { axis: Axis, rpm: f32 },
}

impl Kind {
    /// Whether this node has a shaft end on `face`.
    pub fn has_port(self, face: Face) -> bool {
        match self {
            Kind::Shaft { axis } | Kind::Generator { axis, .. } => face.axis() == axis,
            Kind::Motor { output, .. } => face == output,
        }
    }

    /// The axis the node turns about.
    pub fn axis(self) -> Axis {
        match self {
            Kind::Shaft { axis } | Kind::Generator { axis, .. } => axis,
            Kind::Motor { output, .. } => output.axis(),
        }
    }

    /// The speed this node asks of its network, or `None` when it is not a source.
    pub fn drive(self) -> Option<f32> {
        match self {
            Kind::Shaft { .. } => None,
            Kind::Motor { rpm, .. } | Kind::Generator { rpm, .. } => Some(rpm),
        }
    }
}

/// How the sources of a network relate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    /// No source: nothing turns.
    Idle,
    /// Every source asks for the same speed.
    Running,
    /// Two sources ask for different speeds, so the network locks up.
    Conflict,
}

/// A solved connected component.
#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    /// Every member, sorted by position.
    pub members: Vec<Pos>,
    /// The sources among them, sorted by position.
    pub sources: Vec<Pos>,
    pub status: Status,
    /// Signed speed along the shared axis. Zero unless the network is running.
    pub rpm: f32,
}

/// Solves the network around `start`, or `None` when no node stands there.
///
/// `lookup` reports the node at a position. It is called at most once per position.
pub fn solve(mut lookup: impl FnMut(Pos) -> Option<Kind>, start: Pos) -> Option<Network> {
    let mut seen: BTreeMap<Pos, Kind> = BTreeMap::new();
    let first = lookup(start)?;
    seen.insert(start, first);
    let mut queue = VecDeque::from([start]);
    // Positions already looked up that hold no node, so a miss is not asked twice.
    let mut empty: BTreeSet<Pos> = BTreeSet::new();
    while let Some(here) = queue.pop_front() {
        let kind = seen[&here];
        for face in Face::ALL {
            if !kind.has_port(face) {
                continue;
            }
            let next = face.step(here);
            if seen.contains_key(&next) || empty.contains(&next) {
                continue;
            }
            match lookup(next) {
                Some(neighbour) => {
                    seen.insert(next, neighbour);
                    if neighbour.has_port(face.opposite()) {
                        queue.push_back(next);
                    }
                }
                None => {
                    empty.insert(next);
                }
            }
        }
    }
    // A looked-up neighbour only joins when it points back. Rebuild the component from the
    // pairs that agree, starting at `start`, so a block that merely sits beside a port is out.
    let members = connected(&seen, start);
    let mut sources = Vec::new();
    let mut speeds: Vec<f32> = Vec::new();
    for pos in &members {
        if let Some(rpm) = seen[pos].drive() {
            sources.push(*pos);
            speeds.push(rpm);
        }
    }
    let (status, rpm) = match speeds.split_first() {
        None => (Status::Idle, 0.0),
        Some((first, rest)) if rest.iter().all(|speed| speed == first) => (Status::Running, *first),
        Some(_) => (Status::Conflict, 0.0),
    };
    Some(Network {
        members,
        sources,
        status,
        rpm,
    })
}

/// The positions reachable from `start` through ports that face each other.
fn connected(nodes: &BTreeMap<Pos, Kind>, start: Pos) -> Vec<Pos> {
    let mut members = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(here) = queue.pop_front() {
        let kind = nodes[&here];
        for face in Face::ALL {
            if !kind.has_port(face) {
                continue;
            }
            let next = face.step(here);
            let Some(other) = nodes.get(&next) else {
                continue;
            };
            if other.has_port(face.opposite()) && members.insert(next) {
                queue.push_back(next);
            }
        }
    }
    members.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(nodes: &[(Pos, Kind)]) -> impl FnMut(Pos) -> Option<Kind> + '_ {
        move |pos| nodes.iter().find(|(at, _)| *at == pos).map(|(_, kind)| *kind)
    }

    const EAST: Face = Face([1, 0, 0]);
    const WEST: Face = Face([-1, 0, 0]);
    const UP: Face = Face([0, 1, 0]);

    fn shaft(axis: Axis) -> Kind {
        Kind::Shaft { axis }
    }

    fn motor(output: Face, rpm: f32) -> Kind {
        Kind::Motor { output, rpm }
    }

    #[test]
    fn nothing_stands_at_an_empty_position() {
        assert_eq!(solve(world(&[]), [0, 0, 0]), None);
    }

    #[test]
    fn a_lone_shaft_is_idle() {
        let nodes = [([0, 0, 0], shaft(Axis::X))];
        let network = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(network.members, vec![[0, 0, 0]]);
        assert_eq!(network.status, Status::Idle);
        assert_eq!(network.rpm, 0.0);
    }

    #[test]
    fn a_motor_drives_a_line_of_shafts() {
        let nodes = [
            ([0, 0, 0], motor(EAST, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], shaft(Axis::X)),
            ([3, 0, 0], shaft(Axis::X)),
        ];
        // Every member solves to the same network, whichever one is asked.
        for (pos, _) in &nodes {
            let network = solve(world(&nodes), *pos).unwrap();
            assert_eq!(network.members.len(), 4);
            assert_eq!(network.sources, vec![[0, 0, 0]]);
            assert_eq!(network.status, Status::Running);
            assert_eq!(network.rpm, 16.0);
        }
    }

    #[test]
    fn a_gap_breaks_the_network() {
        let nodes = [
            ([0, 0, 0], motor(EAST, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([3, 0, 0], shaft(Axis::X)),
        ];
        let near = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(near.members.len(), 2);
        let far = solve(world(&nodes), [3, 0, 0]).unwrap();
        assert_eq!(far.members, vec![[3, 0, 0]]);
        assert_eq!(far.status, Status::Idle);
    }

    #[test]
    fn a_shaft_does_not_join_a_neighbour_beside_its_side() {
        // The second shaft lies on Y, so it has no port toward the first one's end.
        let nodes = [([0, 0, 0], shaft(Axis::X)), ([1, 0, 0], shaft(Axis::Y))];
        let network = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(network.members, vec![[0, 0, 0]]);
    }

    #[test]
    fn a_motor_facing_away_drives_nothing() {
        let nodes = [([0, 0, 0], motor(WEST, 16.0)), ([1, 0, 0], shaft(Axis::X))];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.members, vec![[1, 0, 0]]);
        assert_eq!(network.status, Status::Idle);
    }

    #[test]
    fn a_motor_only_connects_through_its_output() {
        // A shaft above a motor that faces east is beside its side, not its output.
        let nodes = [([0, 0, 0], motor(EAST, 16.0)), ([0, 1, 0], shaft(Axis::Y))];
        let network = solve(world(&nodes), [0, 1, 0]).unwrap();
        assert_eq!(network.members, vec![[0, 1, 0]]);
        assert_eq!(UP.axis(), Axis::Y);
    }

    #[test]
    fn two_sources_with_the_same_speed_agree() {
        let nodes = [
            ([0, 0, 0], motor(EAST, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], motor(WEST, 16.0)),
        ];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.sources.len(), 2);
        assert_eq!(network.status, Status::Running);
        assert_eq!(network.rpm, 16.0);
    }

    #[test]
    fn two_sources_with_different_speeds_conflict() {
        let nodes = [
            ([0, 0, 0], motor(EAST, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], motor(WEST, 32.0)),
        ];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.status, Status::Conflict);
        assert_eq!(network.rpm, 0.0);
        // Opposite signs also disagree: they would turn the line both ways.
        let reversed = [
            ([0, 0, 0], motor(EAST, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], motor(WEST, -16.0)),
        ];
        assert_eq!(
            solve(world(&reversed), [1, 0, 0]).unwrap().status,
            Status::Conflict
        );
    }

    #[test]
    fn a_negative_speed_is_kept_with_its_sign() {
        let nodes = [([0, 0, 0], motor(EAST, -64.0)), ([1, 0, 0], shaft(Axis::X))];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.rpm, -64.0);
    }

    #[test]
    fn every_position_is_looked_up_at_most_once() {
        let nodes = [
            ([0, 0, 0], motor(EAST, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let mut calls: Vec<Pos> = Vec::new();
        let mut inner = world(&nodes);
        let network = solve(
            |pos| {
                calls.push(pos);
                inner(pos)
            },
            [0, 0, 0],
        )
        .unwrap();
        assert_eq!(network.members.len(), 3);
        let mut unique = calls.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), calls.len());
    }

    fn generator(axis: Axis, rpm: f32) -> Kind {
        Kind::Generator { axis, rpm }
    }

    #[test]
    fn a_generator_drives_both_sides() {
        let nodes = [
            ([0, 0, 0], shaft(Axis::X)),
            ([1, 0, 0], generator(Axis::X, 16.0)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        for (pos, _) in &nodes {
            let network = solve(world(&nodes), *pos).unwrap();
            assert_eq!(network.members.len(), 3);
            assert_eq!(network.sources, vec![[1, 0, 0]]);
            assert_eq!(network.status, Status::Running);
            assert_eq!(network.rpm, 16.0);
        }
    }

    #[test]
    fn a_generator_only_joins_along_its_axis() {
        // A shaft on Y beside an X generator touches its side, so they stay apart.
        let nodes = [
            ([0, 0, 0], generator(Axis::X, 16.0)),
            ([0, 1, 0], shaft(Axis::Y)),
            ([0, 0, 1], shaft(Axis::Z)),
        ];
        assert_eq!(solve(world(&nodes), [0, 1, 0]).unwrap().members.len(), 1);
        assert_eq!(solve(world(&nodes), [0, 0, 1]).unwrap().members.len(), 1);
        assert_eq!(solve(world(&nodes), [0, 0, 0]).unwrap().members.len(), 1);
    }

    #[test]
    fn a_generator_and_a_motor_can_share_a_network() {
        let nodes = [
            ([0, 0, 0], motor(EAST, 32.0)),
            ([1, 0, 0], generator(Axis::X, 32.0)),
        ];
        let network = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(network.sources.len(), 2);
        assert_eq!(network.status, Status::Running);
        assert_eq!(network.rpm, 32.0);
        let clash = [
            ([0, 0, 0], motor(EAST, 32.0)),
            ([1, 0, 0], generator(Axis::X, 16.0)),
        ];
        assert_eq!(
            solve(world(&clash), [0, 0, 0]).unwrap().status,
            Status::Conflict
        );
    }

    #[test]
    fn drive_reports_only_sources() {
        assert_eq!(shaft(Axis::Y).drive(), None);
        assert_eq!(motor(UP, 8.0).drive(), Some(8.0));
        assert_eq!(generator(Axis::Z, -4.0).drive(), Some(-4.0));
        assert_eq!(generator(Axis::Z, -4.0).axis(), Axis::Z);
    }

    #[test]
    fn long_lines_are_solved() {
        let mut nodes = vec![([0, 0, 0], motor(EAST, 8.0))];
        nodes.extend((1..=200).map(|x| ([x, 0, 0], shaft(Axis::X))));
        let network = solve(world(&nodes), [200, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 201);
        assert_eq!(network.rpm, 8.0);
    }
}

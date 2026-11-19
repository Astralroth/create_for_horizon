//! The kinetic network solver: pure logic with no game types, so it can be tested alone.
//!
//! The game adapter describes the world through a lookup from a position to the [`Kind`]
//! standing there. [`solve`] walks the connected component around a position, works out how
//! fast every block turns and in which direction, and checks that the sources agree and that
//! the machines do not ask for more than the sources give.
//!
//! Every block turns about one axis and has a shaft end at both ends of it. Two blocks are
//! linked by one of two rules:
//! * **Shaft link**: neighbours along the shared axis. Speed and direction carry over.
//! * **Mesh**: two cogwheels on the same axis whose teeth touch. The direction reverses, and
//!   the speed changes by the ratio of the radii.
//!
//! Cogwheels come in two sizes. Same-size cogs mesh when they touch edge to edge (orthogonal
//! neighbours in the plane across the axis) at ratio 1. A small and a large cog mesh when they
//! sit corner to corner (diagonal neighbours), and the small one turns twice as fast.
//!
//! Rotation is signed relative to the positive direction of the block's own axis.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A block position.
pub type Pos = [i32; 3];

/// Speeds closer than this are the same speed.
const TOLERANCE: f32 = 1e-3;

/// A spatial axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    /// Index into a [`Pos`].
    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    /// The axis of a unit face vector such as `[0, 1, 0]`, or `None` for anything else.
    pub fn of_face(face: [i32; 3]) -> Option<Axis> {
        let nonzero: Vec<usize> = (0..3).filter(|i| face[*i] != 0).collect();
        match nonzero.as_slice() {
            [0] => Some(Axis::X),
            [1] => Some(Axis::Y),
            [2] => Some(Axis::Z),
            _ => None,
        }
    }

    /// The state value that spells this axis.
    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
            Axis::Z => "z",
        }
    }

    /// Where a block turning about this axis ends up after a quarter turn about `face`, the axis
    /// of the face a wrench was used on. Turning about another axis takes the third axis; a face
    /// on the block's own axis cannot turn it, so the axis steps on, X to Y to Z and back.
    pub fn turned_about(self, face: Axis) -> Axis {
        if face == self {
            return match self {
                Axis::X => Axis::Y,
                Axis::Y => Axis::Z,
                Axis::Z => Axis::X,
            };
        }
        Axis::ALL
            .into_iter()
            .find(|axis| *axis != self && *axis != face)
            .unwrap_or(self)
    }
}

/// What a block does in a network.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A plain shaft.
    Shaft { axis: Axis },
    /// A cogwheel. `large` cogs have twice the radius of small ones.
    Cog { axis: Axis, large: bool },
    /// A source. `rpm` is signed along `axis`; it can carry `capacity` stress units per RPM.
    Generator {
        axis: Axis,
        rpm: f32,
        capacity: f32,
    },
    /// A machine that uses `impact` stress units for every RPM it turns at.
    Consumer { axis: Axis, impact: f32 },
}

impl Kind {
    /// The axis the block turns about.
    pub fn axis(self) -> Axis {
        match self {
            Kind::Shaft { axis }
            | Kind::Cog { axis, .. }
            | Kind::Generator { axis, .. }
            | Kind::Consumer { axis, .. } => axis,
        }
    }

    /// The speed this block asks of its network, or `None` when it is not a source.
    pub fn drive(self) -> Option<f32> {
        match self {
            Kind::Generator { rpm, .. } => Some(rpm),
            _ => None,
        }
    }
}

/// The signed factor `speed(b) / speed(a)` when the block `a` at `from` is linked to `b` at
/// `to`, or `None` when they are not linked. The rule is symmetric: `link(b, a)` is the
/// reciprocal.
pub fn link(a: Kind, from: Pos, b: Kind, to: Pos) -> Option<f32> {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let axis = a.axis();
    if axis != b.axis() {
        return None;
    }
    let along = delta[axis.index()];
    let across: Vec<i32> = Axis::ALL
        .into_iter()
        .filter(|other| *other != axis)
        .map(|other| delta[other.index()])
        .collect();
    // Shaft link: one step along the shared axis.
    if along.abs() == 1 && across.iter().all(|step| *step == 0) {
        return Some(1.0);
    }
    // Mesh: both are cogs in the same plane.
    if along != 0 {
        return None;
    }
    let (Kind::Cog { large: a_large, .. }, Kind::Cog { large: b_large, .. }) = (a, b) else {
        return None;
    };
    let touching = across.iter().filter(|step| **step != 0).count();
    let unit = across.iter().all(|step| step.abs() <= 1);
    match (touching, unit, a_large == b_large) {
        // Edge to edge, same size.
        (1, true, true) => Some(-1.0),
        // Corner to corner, different sizes: the larger wheel turns slower.
        (2, true, false) => Some(if a_large { -2.0 } else { -0.5 }),
        _ => None,
    }
}

/// Where a network's blocks can lie relative to a block: its two axis ends and, for a cog, the
/// eight cells around it in the plane across its axis.
fn candidates(kind: Kind, pos: Pos) -> Vec<Pos> {
    let axis = kind.axis();
    let mut cells = Vec::new();
    for step in [-1, 1] {
        let mut next = pos;
        next[axis.index()] += step;
        cells.push(next);
    }
    if matches!(kind, Kind::Cog { .. }) {
        let planar: Vec<Axis> = Axis::ALL.into_iter().filter(|a| *a != axis).collect();
        for first in -1..=1 {
            for second in -1..=1 {
                if first == 0 && second == 0 {
                    continue;
                }
                let mut next = pos;
                next[planar[0].index()] += first;
                next[planar[1].index()] += second;
                cells.push(next);
            }
        }
    }
    cells
}

/// How a network ended up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    /// No source: nothing turns.
    Idle,
    /// Everything turns.
    Running,
    /// The sources, or a loop of cogs, ask for speeds that cannot all hold: the network locks.
    Conflict,
    /// The machines use more stress than the sources can carry: the network stops.
    Overstressed,
}

/// One block of a solved network.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Member {
    pub pos: Pos,
    pub kind: Kind,
    /// Signed speed about the block's own axis. Zero unless the network is running.
    pub rpm: f32,
}

/// A solved connected component.
#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    /// Every block, sorted by position.
    pub members: Vec<Member>,
    pub status: Status,
    /// Stress units the machines would use at the speeds the sources impose.
    pub stress: f32,
    /// Stress units the sources can carry.
    pub capacity: f32,
}

impl Network {
    /// The member at `pos`.
    pub fn member(&self, pos: Pos) -> Option<&Member> {
        self.members.iter().find(|member| member.pos == pos)
    }

    /// The sources among the members.
    #[allow(dead_code)]
    pub fn sources(&self) -> impl Iterator<Item = &Member> {
        self.members
            .iter()
            .filter(|member| member.kind.drive().is_some())
    }
}

/// Solves the network around `start`, or `None` when nothing stands there.
///
/// `lookup` reports the block at a position. It is called at most once per position.
pub fn solve(mut lookup: impl FnMut(Pos) -> Option<Kind>, start: Pos) -> Option<Network> {
    let first = lookup(start)?;
    let mut kinds: BTreeMap<Pos, Kind> = BTreeMap::from([(start, first)]);
    let mut empty: BTreeSet<Pos> = BTreeSet::new();
    // Every linked pair, with the factor from the first to the second.
    let mut edges: BTreeMap<Pos, Vec<(Pos, f32)>> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut visited = BTreeSet::from([start]);
    while let Some(here) = queue.pop_front() {
        let kind = kinds[&here];
        for next in candidates(kind, here) {
            if empty.contains(&next) {
                continue;
            }
            let other = match kinds.get(&next) {
                Some(other) => *other,
                None => match lookup(next) {
                    Some(other) => {
                        kinds.insert(next, other);
                        other
                    }
                    None => {
                        empty.insert(next);
                        continue;
                    }
                },
            };
            let Some(factor) = link(kind, here, other, next) else {
                continue;
            };
            edges.entry(here).or_default().push((next, factor));
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    // The component: every position that was reached through a link.
    let component: Vec<Pos> = visited.iter().copied().collect();
    let sources: Vec<Pos> = component
        .iter()
        .copied()
        .filter(|pos| kinds[pos].drive().is_some())
        .collect();

    let mut speeds: BTreeMap<Pos, f32> = BTreeMap::new();
    let mut status = Status::Idle;
    if let Some(reference) = sources.first().copied() {
        status = Status::Running;
        let seed = kinds[&reference].drive().unwrap_or(0.0);
        speeds.insert(reference, seed);
        let mut queue = VecDeque::from([reference]);
        'walk: while let Some(here) = queue.pop_front() {
            let speed = speeds[&here];
            for (next, factor) in edges.get(&here).map(Vec::as_slice).unwrap_or(&[]) {
                let expected = speed * factor;
                match speeds.get(next) {
                    Some(have) if (have - expected).abs() > TOLERANCE => {
                        status = Status::Conflict;
                        break 'walk;
                    }
                    Some(_) => {}
                    None => {
                        speeds.insert(*next, expected);
                        queue.push_back(*next);
                    }
                }
            }
        }
        // Every source must hold the speed the walk assigned to it.
        if status == Status::Running {
            for source in &sources {
                let wanted = kinds[source].drive().unwrap_or(0.0);
                let got = speeds.get(source).copied().unwrap_or(0.0);
                if (wanted - got).abs() > TOLERANCE {
                    status = Status::Conflict;
                    break;
                }
            }
        }
    }

    let mut stress = 0.0;
    let mut capacity = 0.0;
    if status == Status::Running {
        for pos in &component {
            let speed = speeds.get(pos).copied().unwrap_or(0.0).abs();
            match kinds[pos] {
                Kind::Consumer { impact, .. } => stress += impact * speed,
                Kind::Generator { capacity: per_rpm, .. } => capacity += per_rpm * speed,
                _ => {}
            }
        }
        if stress > capacity {
            status = Status::Overstressed;
        }
    }

    let turning = status == Status::Running;
    let members = component
        .iter()
        .map(|pos| Member {
            pos: *pos,
            kind: kinds[pos],
            rpm: if turning {
                speeds.get(pos).copied().unwrap_or(0.0)
            } else {
                0.0
            },
        })
        .collect();
    Some(Network {
        members,
        status,
        stress,
        capacity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(nodes: &[(Pos, Kind)]) -> impl FnMut(Pos) -> Option<Kind> + '_ {
        move |pos| nodes.iter().find(|(at, _)| *at == pos).map(|(_, kind)| *kind)
    }

    fn shaft(axis: Axis) -> Kind {
        Kind::Shaft { axis }
    }
    fn cog(axis: Axis, large: bool) -> Kind {
        Kind::Cog { axis, large }
    }
    fn motor(axis: Axis, rpm: f32) -> Kind {
        Kind::Generator {
            axis,
            rpm,
            capacity: 100.0,
        }
    }
    fn consumer(axis: Axis, impact: f32) -> Kind {
        Kind::Consumer { axis, impact }
    }

    fn rpm(network: &Network, pos: Pos) -> f32 {
        network.member(pos).unwrap().rpm
    }

    #[test]
    fn nothing_stands_at_an_empty_position() {
        assert_eq!(solve(world(&[]), [0, 0, 0]), None);
    }

    #[test]
    fn a_lone_shaft_is_idle() {
        let nodes = [([0, 0, 0], shaft(Axis::X))];
        let network = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 1);
        assert_eq!(network.status, Status::Idle);
        assert_eq!(rpm(&network, [0, 0, 0]), 0.0);
    }

    #[test]
    fn a_generator_drives_a_line_of_shafts_on_both_sides() {
        let nodes = [
            ([0, 0, 0], shaft(Axis::X)),
            ([1, 0, 0], motor(Axis::X, 16.0)),
            ([2, 0, 0], shaft(Axis::X)),
            ([3, 0, 0], shaft(Axis::X)),
        ];
        for (pos, _) in &nodes {
            let network = solve(world(&nodes), *pos).unwrap();
            assert_eq!(network.members.len(), 4);
            assert_eq!(network.status, Status::Running);
            for (at, _) in &nodes {
                assert_eq!(rpm(&network, *at), 16.0);
            }
        }
    }

    #[test]
    fn a_gap_breaks_the_network() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([3, 0, 0], shaft(Axis::X)),
        ];
        assert_eq!(solve(world(&nodes), [1, 0, 0]).unwrap().members.len(), 2);
        let far = solve(world(&nodes), [3, 0, 0]).unwrap();
        assert_eq!(far.members.len(), 1);
        assert_eq!(far.status, Status::Idle);
    }

    #[test]
    fn blocks_on_other_axes_are_not_linked_by_a_shaft_end() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([0, 1, 0], shaft(Axis::Y)),
            ([0, 0, 1], shaft(Axis::Z)),
            ([1, 0, 0], shaft(Axis::Y)),
        ];
        assert_eq!(solve(world(&nodes), [0, 0, 0]).unwrap().members.len(), 1);
    }

    #[test]
    fn two_sources_with_the_same_speed_agree() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], motor(Axis::X, 16.0)),
        ];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.sources().count(), 2);
        assert_eq!(network.status, Status::Running);
    }

    #[test]
    fn sources_with_different_speeds_or_signs_conflict() {
        for other in [32.0, -16.0] {
            let nodes = [
                ([0, 0, 0], motor(Axis::X, 16.0)),
                ([1, 0, 0], shaft(Axis::X)),
                ([2, 0, 0], motor(Axis::X, other)),
            ];
            let network = solve(world(&nodes), [1, 0, 0]).unwrap();
            assert_eq!(network.status, Status::Conflict);
            assert!(network.members.iter().all(|member| member.rpm == 0.0));
        }
    }

    #[test]
    fn a_negative_speed_keeps_its_sign() {
        let nodes = [([0, 0, 0], motor(Axis::X, -64.0)), ([1, 0, 0], shaft(Axis::X))];
        assert_eq!(rpm(&solve(world(&nodes), [1, 0, 0]).unwrap(), [1, 0, 0]), -64.0);
    }

    #[test]
    fn same_size_cogs_mesh_edge_to_edge_and_reverse() {
        // Cogs on Y lie in the XZ plane: neighbours along X or Z touch edge to edge. The motor
        // sits on the shaft end of the first cog.
        let cogs = [
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, false)),
            ([0, 1, 0], motor(Axis::Y, 16.0)),
        ];
        let network = solve(world(&cogs), [1, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 3);
        assert_eq!(rpm(&network, [0, 0, 0]), 16.0);
        assert_eq!(rpm(&network, [1, 0, 0]), -16.0);
    }

    #[test]
    fn a_chain_of_cogs_alternates_direction() {
        let mut nodes = vec![([0, 1, 0], motor(Axis::Y, 8.0))];
        nodes.extend((0..5).map(|x| ([x, 0, 0], cog(Axis::Y, false))));
        let network = solve(world(&nodes), [4, 0, 0]).unwrap();
        for x in 0..5 {
            let expected = if x % 2 == 0 { 8.0 } else { -8.0 };
            assert_eq!(rpm(&network, [x, 0, 0]), expected, "cog {x}");
        }
    }

    #[test]
    fn a_small_and_a_large_cog_mesh_corner_to_corner_at_two_to_one() {
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, true)),
        ];
        let network = solve(world(&nodes), [1, 0, 1]).unwrap();
        assert_eq!(rpm(&network, [0, 0, 0]), 16.0);
        // The large wheel turns half as fast, and the other way.
        assert_eq!(rpm(&network, [1, 0, 1]), -8.0);
        // Seen from the large wheel's side it is the same network.
        let from_small = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(rpm(&from_small, [1, 0, 1]), -8.0);
    }

    #[test]
    fn a_small_and_a_large_cog_do_not_mesh_edge_to_edge() {
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, true)),
        ];
        assert_eq!(solve(world(&nodes), [1, 0, 0]).unwrap().members.len(), 1);
    }

    #[test]
    fn same_size_cogs_do_not_mesh_corner_to_corner() {
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, false)),
        ];
        assert_eq!(solve(world(&nodes), [1, 0, 1]).unwrap().members.len(), 1);
    }

    #[test]
    fn cogs_on_different_axes_do_not_mesh() {
        let nodes = [
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Z, false)),
        ];
        assert_eq!(solve(world(&nodes), [0, 0, 0]).unwrap().members.len(), 1);
    }

    #[test]
    fn a_shaft_through_a_cog_carries_speed_on() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], cog(Axis::X, false)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(rpm(&network, [2, 0, 0]), 16.0);
    }

    #[test]
    fn a_gear_train_changes_speed_by_each_ratio() {
        // Motor shaft through a small cog, which drives a large cog, which has a shaft out.
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 32.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, true)),
            ([1, -1, 1], shaft(Axis::Y)),
        ];
        let network = solve(world(&nodes), [1, -1, 1]).unwrap();
        assert_eq!(rpm(&network, [1, -1, 1]), -16.0);
    }

    #[test]
    fn a_square_of_four_cogs_is_an_even_ring_and_turns() {
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, false)),
            ([0, 0, 1], cog(Axis::Y, false)),
        ];
        let network = solve(world(&nodes), [1, 0, 1]).unwrap();
        assert_eq!(network.status, Status::Running);
        // The opposite corner turns the same way as the driven cog.
        assert_eq!(rpm(&network, [0, 0, 0]), 16.0);
        assert_eq!(rpm(&network, [1, 0, 1]), 16.0);
        assert_eq!(rpm(&network, [1, 0, 0]), -16.0);
        assert_eq!(rpm(&network, [0, 0, 1]), -16.0);
    }

    #[test]
    fn a_ring_through_a_large_cog_stays_consistent() {
        // Three small cogs in a row and a large cog corner to corner with both ends: the loop
        // small - small - small - large - small has ratios that cancel, so it turns.
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, false)),
            ([2, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, true)),
        ];
        let network = solve(world(&nodes), [1, 0, 1]).unwrap();
        assert_eq!(network.status, Status::Running);
        assert_eq!(rpm(&network, [1, 0, 1]), -8.0);
        assert_eq!(rpm(&network, [2, 0, 0]), 16.0);
    }

    #[test]
    fn a_second_source_that_disagrees_through_a_chain_locks_the_network() {
        // The chain drives the cog at (2,0,0) to +16, but its own motor wants -16.
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, false)),
            ([2, 1, 0], motor(Axis::Y, -16.0)),
            ([2, 0, 0], cog(Axis::Y, false)),
        ];
        assert_eq!(solve(world(&nodes), [1, 0, 0]).unwrap().status, Status::Conflict);
    }

    #[test]
    fn a_second_source_that_agrees_through_a_chain_runs() {
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, false)),
            ([2, 1, 0], motor(Axis::Y, 16.0)),
            ([2, 0, 0], cog(Axis::Y, false)),
        ];
        assert_eq!(solve(world(&nodes), [1, 0, 0]).unwrap().status, Status::Running);
    }

    #[test]
    fn stress_within_capacity_runs() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 10.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], consumer(Axis::X, 50.0)),
        ];
        let network = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(network.status, Status::Running);
        assert_eq!(network.stress, 500.0);
        assert_eq!(network.capacity, 1000.0);
    }

    #[test]
    fn stress_above_capacity_stops_the_network() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 10.0)),
            ([1, 0, 0], consumer(Axis::X, 60.0)),
            ([2, 0, 0], consumer(Axis::X, 60.0)),
        ];
        let network = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(network.status, Status::Overstressed);
        assert!(network.stress > network.capacity);
        assert!(network.members.iter().all(|member| member.rpm == 0.0));
    }

    #[test]
    fn a_slow_machine_uses_less_stress_through_gearing() {
        // The same machine at half the speed uses half the stress.
        let direct = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], consumer(Axis::X, 10.0)),
        ];
        assert_eq!(solve(world(&direct), [1, 0, 0]).unwrap().stress, 160.0);
        let geared = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, true)),
            ([1, -1, 1], consumer(Axis::Y, 10.0)),
        ];
        assert_eq!(solve(world(&geared), [1, -1, 1]).unwrap().stress, 80.0);
    }

    #[test]
    fn a_quarter_turn_about_another_axis_takes_the_third_axis() {
        assert_eq!(Axis::X.turned_about(Axis::Y), Axis::Z);
        assert_eq!(Axis::X.turned_about(Axis::Z), Axis::Y);
        assert_eq!(Axis::Y.turned_about(Axis::X), Axis::Z);
        assert_eq!(Axis::Y.turned_about(Axis::Z), Axis::X);
        assert_eq!(Axis::Z.turned_about(Axis::X), Axis::Y);
        assert_eq!(Axis::Z.turned_about(Axis::Y), Axis::X);
    }

    #[test]
    fn a_face_on_the_blocks_own_axis_steps_the_axis_on() {
        assert_eq!(Axis::X.turned_about(Axis::X), Axis::Y);
        assert_eq!(Axis::Y.turned_about(Axis::Y), Axis::Z);
        assert_eq!(Axis::Z.turned_about(Axis::Z), Axis::X);
    }

    #[test]
    fn every_wrench_click_changes_the_axis_and_never_lands_on_the_clicked_one() {
        for start in Axis::ALL {
            for face in Axis::ALL {
                let turned = start.turned_about(face);
                assert_ne!(turned, start, "start {start:?}, face {face:?}");
                assert_ne!(turned, face, "start {start:?}, face {face:?}");
            }
        }
    }

    #[test]
    fn repeated_clicks_on_one_face_alternate_between_the_other_two_axes() {
        // Turning about X can only put a block on Y or Z, so clicking the same X face again and
        // again flips between them. Another face is needed to bring the X axis back.
        let mut axis = Axis::Y;
        let mut seen = Vec::new();
        for _ in 0..4 {
            axis = axis.turned_about(Axis::X);
            seen.push(axis);
        }
        assert_eq!(seen, [Axis::Z, Axis::Y, Axis::Z, Axis::Y]);
        assert_eq!(Axis::Y.turned_about(Axis::Z), Axis::X);
    }

    #[test]
    fn faces_map_to_their_axis_and_names_round_trip() {
        assert_eq!(Axis::of_face([1, 0, 0]), Some(Axis::X));
        assert_eq!(Axis::of_face([0, -1, 0]), Some(Axis::Y));
        assert_eq!(Axis::of_face([0, 0, 1]), Some(Axis::Z));
        assert_eq!(Axis::of_face([0, 0, 0]), None);
        assert_eq!(Axis::of_face([1, 1, 0]), None);
        assert_eq!(Axis::X.name(), "x");
        assert_eq!(Axis::Y.name(), "y");
        assert_eq!(Axis::Z.name(), "z");
    }

    #[test]
    fn link_is_symmetric_with_reciprocal_factors() {
        let small = cog(Axis::Y, false);
        let large = cog(Axis::Y, true);
        let a = [0, 0, 0];
        let b = [1, 0, 1];
        assert_eq!(link(small, a, large, b), Some(-0.5));
        assert_eq!(link(large, b, small, a), Some(-2.0));
        assert_eq!(link(small, a, small, [1, 0, 0]), Some(-1.0));
        assert_eq!(link(small, a, small, [2, 0, 0]), None);
        assert_eq!(link(shaft(Axis::X), a, shaft(Axis::X), [1, 0, 0]), Some(1.0));
        assert_eq!(link(shaft(Axis::X), a, shaft(Axis::Y), [1, 0, 0]), None);
    }

    #[test]
    fn every_position_is_looked_up_at_most_once() {
        let nodes = [
            ([0, 1, 0], motor(Axis::Y, 16.0)),
            ([0, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 0], cog(Axis::Y, false)),
            ([1, 0, 1], cog(Axis::Y, false)),
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
        assert_eq!(network.members.len(), 4);
        let mut unique = calls.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), calls.len());
    }

    #[test]
    fn long_lines_are_solved() {
        let mut nodes = vec![([0, 0, 0], motor(Axis::X, 8.0))];
        nodes.extend((1..=200).map(|x| ([x, 0, 0], shaft(Axis::X))));
        let network = solve(world(&nodes), [200, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 201);
        assert_eq!(rpm(&network, [200, 0, 0]), 8.0);
    }
}

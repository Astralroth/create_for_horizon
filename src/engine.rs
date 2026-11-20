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
    /// A gearbox: shaft ends on the four faces across `axis`. It turns shafts at right angles
    /// to each other, as bevel gears do, and reverses a shaft that runs straight through it.
    /// Its own speed is the one every port would turn at about its own outward direction,
    /// with the second of the two port axes turning the opposite way round.
    Gearbox { axis: Axis },
    /// A shaft that a redstone signal can cut. Engaged it is a shaft; disengaged it has no shaft
    /// ends, so the two sides it joined become separate networks.
    Clutch { axis: Axis, engaged: bool },
    /// A shaft that a redstone signal can reverse. Its own speed is the one at its negative end;
    /// when `reversed` the positive end turns the other way.
    Gearshift { axis: Axis, reversed: bool },
    /// A source with a shaft end on one side only: `side` is `1` for the positive end of `axis`
    /// and `-1` for the negative one. A hand crank, whose other end is the handle. It drives its
    /// network at `rpm` (signed along `axis`) and carries `capacity` stress units per RPM; with
    /// `rpm` zero it is only a joint and does not count as a source.
    Stub {
        axis: Axis,
        side: i32,
        rpm: f32,
        capacity: f32,
    },
}

/// The six unit steps from a block to its neighbours.
const STEPS: [[i32; 3]; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];

impl Kind {
    /// The axis the block turns about, or for a gearbox the axis its ports lie across.
    pub fn axis(self) -> Axis {
        match self {
            Kind::Shaft { axis }
            | Kind::Cog { axis, .. }
            | Kind::Generator { axis, .. }
            | Kind::Consumer { axis, .. }
            | Kind::Gearbox { axis }
            | Kind::Clutch { axis, .. }
            | Kind::Gearshift { axis, .. }
            | Kind::Stub { axis, .. } => axis,
        }
    }

    /// The speed a shaft ending at the port facing `step` turns at, as a factor of this block's
    /// own speed and in the signed convention of a shaft on that step's axis (positive is
    /// counterclockwise seen from the positive end of the axis). `None` when no shaft end faces
    /// that way, which also leaves a disengaged clutch with no ports at all.
    pub fn port_factor(self, step: [i32; 3]) -> Option<f32> {
        if !STEPS.contains(&step) {
            return None;
        }
        let face = Axis::of_face(step)?;
        let sign = if step[face.index()] > 0 { 1.0 } else { -1.0 };
        match self {
            Kind::Shaft { axis }
            | Kind::Cog { axis, .. }
            | Kind::Generator { axis, .. }
            | Kind::Consumer { axis, .. } => (face == axis).then_some(1.0),
            Kind::Clutch { axis, engaged } => (engaged && face == axis).then_some(1.0),
            Kind::Stub { axis, side, .. } => {
                (face == axis && (sign as i32) == side).then_some(1.0)
            }
            Kind::Gearshift { axis, reversed } => {
                (face == axis).then_some(if reversed && sign > 0.0 { -1.0 } else { 1.0 })
            }
            Kind::Gearbox { axis } => {
                if face == axis {
                    return None;
                }
                // Of the two port axes the first turns with the sign of its outward direction
                // and the second the opposite way, which is what bevel gears do at right angles.
                let first = Axis::ALL.into_iter().find(|other| *other != axis)?;
                Some(if face == first { sign } else { -sign })
            }
        }
    }

    /// The speed this block asks of its network, or `None` when it is not a source.
    pub fn drive(self) -> Option<f32> {
        match self {
            Kind::Generator { rpm, .. } => Some(rpm),
            Kind::Stub { rpm, .. } => (rpm != 0.0).then_some(rpm),
            _ => None,
        }
    }
}

/// The signed factor `speed(b) / speed(a)` when the block `a` at `from` is linked to `b` at
/// `to`, or `None` when they are not linked. The rule is symmetric: `link(b, a)` is the
/// reciprocal.
pub fn link(a: Kind, from: Pos, b: Kind, to: Pos) -> Option<f32> {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    // Shaft link: neighbours with a shaft end facing each other. Both ends sit on one shaft
    // line, so they turn alike about it: speed(a) * port(a) = speed(b) * port(b).
    if STEPS.contains(&delta) {
        let back = [-delta[0], -delta[1], -delta[2]];
        if let (Some(from_port), Some(to_port)) = (a.port_factor(delta), b.port_factor(back)) {
            return Some(from_port * to_port);
        }
        // Without shaft ends facing each other the cells can still touch as meshing cogs.
    }
    // Mesh: both are cogs on the same axis, in the same plane.
    let axis = a.axis();
    if axis != b.axis() {
        return None;
    }
    let along = delta[axis.index()];
    if along != 0 {
        return None;
    }
    let across: Vec<i32> = Axis::ALL
        .into_iter()
        .filter(|other| *other != axis)
        .map(|other| delta[other.index()])
        .collect();
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

/// Where a network's blocks can lie relative to a block: the cell in front of each of its shaft
/// ends and, for a cog, the eight cells around it in the plane across its axis.
fn candidates(kind: Kind, pos: Pos) -> Vec<Pos> {
    let axis = kind.axis();
    let mut cells: Vec<Pos> = STEPS
        .into_iter()
        .filter(|step| kind.port_factor(*step).is_some())
        .map(|step| [pos[0] + step[0], pos[1] + step[1], pos[2] + step[2]])
        .collect();
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
                Kind::Generator { capacity: per_rpm, .. }
                | Kind::Stub { capacity: per_rpm, .. } => capacity += per_rpm * speed,
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

    fn crank(side: i32, rpm: f32) -> Kind {
        Kind::Stub {
            axis: Axis::X,
            side,
            rpm,
            capacity: 100.0,
        }
    }

    #[test]
    fn a_crank_drives_only_what_is_on_its_shaft_side() {
        // A crank at x = 1 with its shaft end on the negative side: the shaft at x = 0 turns,
        // the one on the handle side at x = 2 is not part of its network.
        let nodes = [
            ([0, 0, 0], shaft(Axis::X)),
            ([1, 0, 0], crank(-1, 32.0)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.status, Status::Running);
        assert_eq!(network.members.len(), 2);
        assert_eq!(rpm(&network, [0, 0, 0]), 32.0);
        assert!(network.member([2, 0, 0]).is_none());
        // Seen from the handle side, the lone shaft is a network of its own with no source.
        let lone = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(lone.status, Status::Idle);
        assert_eq!(lone.members.len(), 1);
    }

    #[test]
    fn a_crank_on_the_positive_side_joins_the_shaft_after_it() {
        let nodes = [
            ([0, 0, 0], shaft(Axis::X)),
            ([1, 0, 0], crank(1, 32.0)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 2);
        assert_eq!(rpm(&network, [2, 0, 0]), 32.0);
        assert!(network.member([0, 0, 0]).is_none());
    }

    #[test]
    fn a_crank_carries_its_capacity_and_a_resting_one_is_only_a_joint() {
        let nodes = [
            ([0, 0, 0], consumer(Axis::X, 2.0)),
            ([1, 0, 0], crank(-1, 10.0)),
        ];
        let network = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(network.capacity, 1000.0);
        assert_eq!(network.stress, 20.0);
        // At rest the crank is no source: the network is idle, and a second source can drive it.
        let resting = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], crank(-1, 0.0)),
        ];
        let network = solve(world(&resting), [0, 0, 0]).unwrap();
        assert_eq!(network.status, Status::Running);
        assert_eq!(rpm(&network, [2, 0, 0]), 16.0);
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

    fn gearbox(axis: Axis) -> Kind {
        Kind::Gearbox { axis }
    }
    fn clutch(axis: Axis, engaged: bool) -> Kind {
        Kind::Clutch { axis, engaged }
    }
    fn gearshift(axis: Axis, reversed: bool) -> Kind {
        Kind::Gearshift { axis, reversed }
    }

    #[test]
    fn the_ports_of_shafts_and_cogs_lie_on_their_axis() {
        for kind in [shaft(Axis::X), cog(Axis::X, true), motor(Axis::X, 1.0)] {
            assert_eq!(kind.port_factor([1, 0, 0]), Some(1.0));
            assert_eq!(kind.port_factor([-1, 0, 0]), Some(1.0));
            assert_eq!(kind.port_factor([0, 1, 0]), None);
            assert_eq!(kind.port_factor([0, 0, -1]), None);
        }
        // Anything that is not one unit step has no port.
        assert_eq!(shaft(Axis::X).port_factor([2, 0, 0]), None);
        assert_eq!(shaft(Axis::X).port_factor([1, 1, 0]), None);
        assert_eq!(shaft(Axis::X).port_factor([0, 0, 0]), None);
    }

    #[test]
    fn a_gearbox_has_ports_across_its_axis_with_opposite_polarity_per_axis() {
        // Axis Y: ports on X (first, turns with the outward sign) and Z (the opposite way).
        let g = gearbox(Axis::Y);
        assert_eq!(g.port_factor([1, 0, 0]), Some(1.0));
        assert_eq!(g.port_factor([-1, 0, 0]), Some(-1.0));
        assert_eq!(g.port_factor([0, 0, 1]), Some(-1.0));
        assert_eq!(g.port_factor([0, 0, -1]), Some(1.0));
        assert_eq!(g.port_factor([0, 1, 0]), None);
        assert_eq!(g.port_factor([0, -1, 0]), None);
        // Axis X: ports on Y (first) and Z.
        let g = gearbox(Axis::X);
        assert_eq!(g.port_factor([0, 1, 0]), Some(1.0));
        assert_eq!(g.port_factor([0, 0, 1]), Some(-1.0));
        assert_eq!(g.port_factor([1, 0, 0]), None);
        // Axis Z: ports on X (first) and Y.
        let g = gearbox(Axis::Z);
        assert_eq!(g.port_factor([-1, 0, 0]), Some(-1.0));
        assert_eq!(g.port_factor([0, 1, 0]), Some(-1.0));
        assert_eq!(g.port_factor([0, 0, 1]), None);
    }

    #[test]
    fn a_shaft_straight_through_a_gearbox_comes_out_reversed() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], shaft(Axis::X)),
            ([2, 0, 0], gearbox(Axis::Y)),
            ([3, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [3, 0, 0]).unwrap();
        assert_eq!(network.status, Status::Running);
        assert_eq!(rpm(&network, [1, 0, 0]), 16.0);
        assert_eq!(rpm(&network, [3, 0, 0]), -16.0);
    }

    #[test]
    fn a_gearbox_turns_a_corner_the_way_bevel_gears_do() {
        // A shaft along X drives one along Z. Bevel gears at right angles meet with their
        // outward speeds opposed, which in the world frame is +16 about X and +16 about Z here.
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], gearbox(Axis::Y)),
            ([1, 0, 1], shaft(Axis::Z)),
            ([1, 0, -1], shaft(Axis::Z)),
        ];
        let network = solve(world(&nodes), [1, 0, 1]).unwrap();
        assert_eq!(network.members.len(), 4);
        assert_eq!(rpm(&network, [1, 0, 1]), 16.0);
        // The shaft on the other side of the same axis turns the opposite way round.
        assert_eq!(rpm(&network, [1, 0, -1]), -16.0);
    }

    #[test]
    fn gearboxes_side_by_side_pass_the_speed_on() {
        let nodes = [
            ([-1, 0, 0], motor(Axis::X, 16.0)),
            ([0, 0, 0], gearbox(Axis::Y)),
            ([1, 0, 0], gearbox(Axis::Y)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 4);
        // Each gearbox reverses once, so the shaft ends up turning as the motor does.
        assert_eq!(rpm(&network, [2, 0, 0]), 16.0);
    }

    #[test]
    fn a_gearbox_has_no_port_on_its_own_axis() {
        let nodes = [([0, 0, 0], gearbox(Axis::Y)), ([0, 1, 0], shaft(Axis::Y))];
        assert_eq!(solve(world(&nodes), [0, 1, 0]).unwrap().members.len(), 1);
        let nodes = [([0, 0, 0], gearbox(Axis::Y)), ([0, 0, 0], gearbox(Axis::Y))];
        assert_eq!(solve(world(&nodes), [0, 0, 0]).unwrap().members.len(), 1);
        // Two gearboxes turned differently do not meet through a face one of them lacks.
        let nodes = [([0, 0, 0], gearbox(Axis::Y)), ([1, 0, 0], gearbox(Axis::X))];
        assert_eq!(solve(world(&nodes), [0, 0, 0]).unwrap().members.len(), 1);
    }

    #[test]
    fn an_engaged_clutch_is_a_shaft() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], clutch(Axis::X, true)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(network.members.len(), 3);
        assert_eq!(rpm(&network, [2, 0, 0]), 16.0);
    }

    #[test]
    fn a_disengaged_clutch_cuts_the_network_in_two() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], clutch(Axis::X, false)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let driven = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(driven.members.len(), 1);
        assert_eq!(driven.status, Status::Idle);
        assert_eq!(rpm(&driven, [2, 0, 0]), 0.0);
        let source_side = solve(world(&nodes), [0, 0, 0]).unwrap();
        assert_eq!(source_side.members.len(), 1);
        // The clutch itself belongs to neither side.
        let alone = solve(world(&nodes), [1, 0, 0]).unwrap();
        assert_eq!(alone.members.len(), 1);
        assert_eq!(alone.status, Status::Idle);
    }

    #[test]
    fn a_gearshift_passes_the_speed_on_or_reverses_the_far_side() {
        let build = |reversed: bool| {
            [
                ([0, 0, 0], shaft(Axis::X)),
                ([1, 0, 0], gearshift(Axis::X, reversed)),
                ([2, 0, 0], motor(Axis::X, 16.0)),
            ]
        };
        let plain = build(false);
        let network = solve(world(&plain), [0, 0, 0]).unwrap();
        assert_eq!(rpm(&network, [0, 0, 0]), 16.0);
        // Reversed, the motor sits on the positive end and the shaft on the negative end.
        let reversed = build(true);
        let network = solve(world(&reversed), [0, 0, 0]).unwrap();
        assert_eq!(rpm(&network, [0, 0, 0]), -16.0);
        assert_eq!(network.status, Status::Running);
    }

    #[test]
    fn a_reversed_gearshift_reverses_from_either_side() {
        // The motor on the negative end now: the positive-end shaft comes out reversed.
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], gearshift(Axis::X, true)),
            ([2, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [2, 0, 0]).unwrap();
        assert_eq!(rpm(&network, [0, 0, 0]), 16.0);
        assert_eq!(rpm(&network, [2, 0, 0]), -16.0);
    }

    #[test]
    fn two_reversed_gearshifts_cancel() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 16.0)),
            ([1, 0, 0], gearshift(Axis::X, true)),
            ([2, 0, 0], gearshift(Axis::X, true)),
            ([3, 0, 0], shaft(Axis::X)),
        ];
        let network = solve(world(&nodes), [3, 0, 0]).unwrap();
        assert_eq!(rpm(&network, [3, 0, 0]), 16.0);
    }

    #[test]
    fn a_gearshift_carries_stress_like_a_shaft() {
        let nodes = [
            ([0, 0, 0], motor(Axis::X, 10.0)),
            ([1, 0, 0], gearshift(Axis::X, true)),
            ([2, 0, 0], consumer(Axis::X, 50.0)),
        ];
        let network = solve(world(&nodes), [2, 0, 0]).unwrap();
        // The machine runs the other way round but at the same speed, so the same stress.
        assert_eq!(rpm(&network, [2, 0, 0]), -10.0);
        assert_eq!(network.stress, 500.0);
    }

    #[test]
    fn every_link_has_a_reciprocal_from_the_other_side() {
        let kinds = [
            shaft(Axis::X),
            shaft(Axis::Y),
            shaft(Axis::Z),
            cog(Axis::Y, false),
            cog(Axis::Y, true),
            cog(Axis::Z, false),
            motor(Axis::X, 8.0),
            consumer(Axis::Z, 2.0),
            gearbox(Axis::X),
            gearbox(Axis::Y),
            gearbox(Axis::Z),
            clutch(Axis::X, true),
            clutch(Axis::Y, false),
            gearshift(Axis::X, false),
            gearshift(Axis::Y, true),
        ];
        let mut offsets = Vec::new();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if [x, y, z] != [0, 0, 0] {
                        offsets.push([x, y, z]);
                    }
                }
            }
        }
        let mut linked = 0;
        for a in kinds {
            for b in kinds {
                for offset in &offsets {
                    let forward = link(a, [0, 0, 0], b, *offset);
                    let back = link(b, *offset, a, [0, 0, 0]);
                    match (forward, back) {
                        (None, None) => {}
                        (Some(f), Some(r)) => {
                            linked += 1;
                            assert!((f * r - 1.0).abs() < 1e-6, "{a:?} {b:?} {offset:?}: {f} {r}");
                        }
                        other => panic!("one-sided link {a:?} {b:?} {offset:?}: {other:?}"),
                    }
                }
            }
        }
        assert!(linked > 100, "only {linked} links were exercised");
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

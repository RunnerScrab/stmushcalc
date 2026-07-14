//! Damage output simulator. Runs a simulation of a ship turning and firing all its arcs. How and
//! when it turns depends on the TurnTimingModel: a ship can turn in anticipation of its next ready
//! arc so that that Arc is already pointed when ready to fire, amortizing the the time delay; or it
//! can only start turning once an arc has reported it has recycled.
//!
//! The purpose of this simulator is not to recreate a realistic fight, but to provide a standard
//! benchmark model for damage out that takes having to turn into account. It also only simulates
//! damage output, not damage the target receives (which would be damage input).
//!   
//! There may be inaccuracies because we do not have access to the game's source code (AFAIK),
//! and ship behavior was reverse-engineered using imperfect observations. For example, the data used
//! to infer the MoveRatio turn time relationship has network latency and the server's scheduling
//! baked in, and both are effectively unpredictable from a player standpoint 
//!
//! Model assumptions: 
//!   * There is a single target 
//!
//!   * A ship presents only one arc at a time because there is only a single target
//!
//!   * All weapons start the simulation ready to fire
//!
//!   * The target is stationary relative to the player's ship. Against a moving opponent, the face
//!   commands will track their position and stop at moment it intercepts, meaning their motion can
//!   increase or decrease the required Euler angle delta
//!   
//!   * Model uses no fixed rotation pattern: each step turns to the face whose next weapon recycles
//!   soonest, ties (such as at start, when all weapons are ready) broken by largest weapon group,
//!   then next shortest turn
//!
//!   * Range and falloff is ignored; a weapon fires once recycled and facing the target 
//!
//!   * Adjacent cube faces are 90 degrees from one another; opposite faces are 180
//!
//!   * A player will wait for (or will not be able to react to) weapons recycling within a short
//!   window of each other, e.g. within 0.4 s of each other, and will fire them as a single group.
//!   This makes the damage signal easier to sample, for one, but is also representative of a tactic
//!   that is preferable at least some of the time: you want to maximize your volley sizes to make
//!   sure the damage is put into a single shield facing, rather than distributed across them.

use smallvec::*;
use crate::rng::TurnRng;
use crate::ship::Ship;
use crate::weapon::Weapon;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum Face {
    Fore = 1,
    Aft = 2,
    Port = 4,
    Starboard = 8,
    Dorsal = 16,
    Ventral = 32,
}

impl Face {

    pub const ALL: [Face; 6] = [
        Face::Fore,
        Face::Aft,
        Face::Port,
        Face::Starboard,
        Face::Dorsal,
        Face::Ventral,
    ];

    #[inline]
    const fn from_char(c: char) -> Option<Face> {
        match c.to_ascii_uppercase() {
            'F' => Some(Face::Fore),
            'A' => Some(Face::Aft),
            'P' => Some(Face::Port),
            'S' => Some(Face::Starboard),
            'D' => Some(Face::Dorsal),
            'V' => Some(Face::Ventral),
            _ => None,
        }
    }

    #[inline]
    const fn from_enum_discriminant(b: u8) -> Face {
        unsafe { std::mem::transmute::<u8, Face>(b) }
    }

    /// The face on the opposite side of the cube
    #[inline]
    pub const fn opposite(self) -> Face {
        // Opposite faces are adjacent-bit pairs (F/A, P/S, D/V), so swapping each bit
        // with its neighbor always yields a valid enum discriminant
        let d = self as u8;
        let opp = ((d & 0x55) << 1) | ((d & 0xAA) >> 1);
        unsafe { std::mem::transmute::<u8, Face>(opp) }
    }

    #[inline(always)]
    pub const fn label(self) -> char {
        match self {
            Face::Fore => 'F',
            Face::Aft => 'A',
            Face::Port => 'P',
            Face::Starboard => 'S',
            Face::Dorsal => 'D',
            Face::Ventral => 'V',
        }
    }

    #[inline(always)]
    pub fn angle_to(self, other: Face) -> f64 {
        let same = ((self == other) as u64).wrapping_neg();
        let is_opp = ((self.opposite() == other) as u64).wrapping_neg();
        f64::from_bits(
            (0.0_f64.to_bits() & same)
                | (180.0_f64.to_bits() & is_opp)
                | (90.0_f64.to_bits() & !(same | is_opp)),
        )
    }

    #[inline]
    fn priority(self) -> usize {
        Face::ALL.iter().position(|f| *f == self).unwrap()
    }
}

#[derive(Clone, Copy, Default)]
struct FaceSeq(u64);

impl FaceSeq {
    #[inline]
    fn len(self) -> usize {
        // Faces are nonzero and packed contiguously from the low byte, so the
        // leading zero bytes are the empty slots
        8 - (self.0.leading_zeros() as usize >> 3)
    }

    #[inline]
    fn push(&mut self, f: Face) {
        self.0 |= (f as u64) << (self.len() << 3);
    }

    #[inline]
    fn contains(self, f: Face) -> bool {
        // faces are one-hot bytes, so OR-folding the bytes yields the membership mask
        let x = self.0;
        let x = (x >> 32) | x;
        let x = (x >> 16) | x;
        let x = (x >> 8) | x;
        x as u8 & f as u8 != 0
    }

    #[inline]
    fn push_unique(&mut self, f: Face) {
        if !self.contains(f) {
            self.push(f);
        }
    }

    #[inline]
    fn iter(self) -> impl Iterator<Item = Face> {
        (0..self.len()).map(move |i| Face::from_enum_discriminant((self.0 >> (i << 3)) as u8))
    }
}

/// Bit set of the faces in a weapon's arc string (e.g. "FAPSD"), one-hot per Face
#[inline]
fn arc_mask(arc: &str) -> u8 {
    arc.chars().filter_map(Face::from_char).fold(0u8, |m, f| m | f as u8)
}

/// Per-weapon arc masks, indexed by weapon
#[inline]
fn arc_masks(weapons: &[Weapon]) -> Vec<u8> {
    weapons.iter().map(|w| arc_mask(&w.arc)).collect()
}

/// Whether an arc mask covers a face
#[inline]
fn does_arc_mask_cover_face(arc: u8, face: Face) -> bool {
    arc & face as u8 != 0
}

/// The distinct faces covered by some weapon's arc
fn candidate_faces(weapon_arcs: &[u8]) -> FaceSeq {
    let union = weapon_arcs.iter().fold(0u8, |a, &m| a | m);
    let mut faces = FaceSeq::default();
    for f in Face::ALL {
        if does_arc_mask_cover_face(union, f) {
            faces.push(f);
        }
    }
    faces
}

/// Choose the next face to present and its earliest covering weapon's ready time
fn select_next(
    candidates: FaceSeq,
    weapon_arcs: &[u8],
    next_ready: &[f64],
    orientation: Face,
    t: f64,
    window: f64,
) -> Option<(Face, f64)> {

    let mut best: Option<(Face, f64)> = None;
    let mut best_key: Option<(f64, isize, f64, usize)> = None;
    for f in candidates.iter() {
        let covering = || weapon_arcs.iter().enumerate().filter(|&(_, &arc)| does_arc_mask_cover_face(arc, f));

        let ready = covering()
            .map(|(w, _)| next_ready[w])
            .fold(f64::INFINITY, f64::min);

        if !ready.is_finite() {
            // Every weapon in this arc is spent
            continue; 
        }

        // Weapons ready by the earliest feasible fire time (>= now) plus window
        let fire_by = ready.max(t) + window;
        let group = covering().filter(|(w, _)| next_ready[*w] <= fire_by).count();
        let key = (
            ready,
            -(group as isize),
            orientation.angle_to(f),
            f.priority(),
        );
        if best_key.map_or(true, |bk| key < bk) {
            best_key = Some(key);
            best = Some((f, ready));
        }
    }
    best
}

/// Arrival pushed forward to the latest weapon(s) that recycles within
/// a window of it, e.g. you have missiles that recycle 0.5s after the beams on
/// the same arc. We *assume* you want to fire them as a single group to maximize
/// damage to the current shield face the enemy is presenting
fn consolidate_arc_weapons(
    arrival: f64,
    face: Face,
    weapon_arcs: &[u8],
    next_ready: &[f64],
    window: f64,
) -> f64 {
    let cutoff = arrival + window;
    let mut fire_t = arrival;
    for (i, &arc) in weapon_arcs.iter().enumerate() {
        if does_arc_mask_cover_face(arc, face) && next_ready[i] <= cutoff && next_ready[i] > fire_t {
            fire_t = next_ready[i];
        }
    }
    fire_t
}

/// The adjacent face (90 deg from current face) with the most ready weapons;
/// there will often not be one
fn intermediate_ready_face(
    orientation: Face,
    target: Face,
    candidates: FaceSeq,
    weapon_arcs: &[u8],
    next_ready: &[f64],
    t: f64,
) -> Option<Face> {
    let mut best: Option<(usize, Face)> = None;
    for m in candidates.iter() {
        if orientation.angle_to(m) != 90.0 || m.angle_to(target) != 90.0 {
            continue;
        }
        let exclusive = weapon_arcs
            .iter()
            .enumerate()
            .filter(|&(w, &arc)| {
                does_arc_mask_cover_face(arc, m) &
                    !does_arc_mask_cover_face(arc, target)
                    & (next_ready[w] <= t)
            })
            .count();
        if exclusive > 0 && best.map_or(true, |(c, _)| exclusive > c) {
            best = Some((exclusive, m));
        }
    }
    best.map(|(_, m)| m)
}

/// One weapon discharge
#[derive(Clone, Debug)]
pub struct Fire {
    pub time: f64,
    pub weapon: usize,
    pub face: Face,
    pub damage: f64,
}

/// Models how the pilot begins turning toward the next online arc
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TurnTimingModel {
    /// Start turning early before the next weapon comes online, i.e. no turn delay added;
    /// more representative of when someone else is completely tanking and you can
    /// just dump firepower on target without worrying about being hit
    Anticipatory,

    /// Turn to an arc when its weapons report coming online; 
    /// more representative of actual combat because most players
    /// will turn to distribute incoming damage across shield faces
    Reactive,
}

#[derive(Clone, Debug)]
pub struct SimConfig {
    /// How long to simulate, in seconds
    pub horizon: f64,
    /// Sample spacing of the returned signals, in seconds
    pub t_res: f64,
    /// How the pilot schedules turns relative to weapon recycle
    pub turn_timing: TurnTimingModel,
    /// On reaching an arc, wait up to this many seconds for stragglers so they
    /// fire in one volley. 0.0 fires the instant the first weapon is ready
    pub volley_window: f64,
}

impl SimConfig {
    pub fn new(horizon: f64, dt: f64,
        turn_timing: TurnTimingModel, volley_window: f64) -> Self {
        Self {
            horizon,
            t_res: dt,
            turn_timing,
            volley_window,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DamageSignal {
    pub dt: f64,
    /// Simulated horizon, in seconds
    pub horizon: f64,
    /// Cumulative damage 
    pub cumulative: Vec<(f64, f64)>,
    /// Total damage from start to the horizon 
    pub total: f64,
    /// Largest single-instant, binnned damage
    pub peak: f64,
    /// Every discharge, in time order
    pub events: Vec<Fire>,
    /// The arc rotation the ship goes through 
    pub rotation: SmallVec<[Face; 6]>,
    /// total seconds spent turning between arcs
    pub turn_time_total: f64,
    /// distinct weapon recycle times
    pub recycle_times: Vec<f64>,
}

/// Fire every weapon in arc of face that has recycled by time t
fn fire_ready(
    t: f64,
    face: Face,
    weapon_arcs: &[u8],
    weapons: &[Weapon],
    events: &mut Vec<Fire>,
    next_ready: &mut [f64],
) {
    for (i, &arc) in weapon_arcs.iter().enumerate() {
        if does_arc_mask_cover_face(arc, face) && next_ready[i] <= t {
            events.push(Fire {
                time: t,
                weapon: i,
                face,
                damage: weapons[i].damage,
            });
            next_ready[i] = if weapons[i].recycle_time > 0.0 {
                t + weapons[i].recycle_time
            } else {
                f64::INFINITY
            };
        }
    }
}

/// Fire time of a volley reached by turning, under a turn-timing model
fn arrival_time(timing: TurnTimingModel, t: f64, turn: f64, ready: f64) -> f64 {
    match timing {
        TurnTimingModel::Anticipatory => (t + turn).max(ready),
        TurnTimingModel::Reactive => t.max(ready) + turn,
    }
}

/// Simulate a ship's damage output against a single, in-range enemy over
/// cfg.horizon seconds and return the sampled damage signal
pub fn simulate_damage(ship: &Ship, rng: &mut TurnRng, cfg: &SimConfig) -> DamageSignal {
    let effective;
    let ship = if ship.character.is_some() {
        effective = ship.effective();
        &effective
    } else {
        ship
    };
    let weapons = &ship.weapons;
    let weapon_arcs = arc_masks(weapons);
    let candidates = candidate_faces(&weapon_arcs);

    let pointcount = (cfg.horizon / cfg.t_res) as usize;
    let mut events: Vec<Fire> = Vec::with_capacity(pointcount);
    let mut next_ready = vec![0.0f64; weapons.len()];
    let horizon = cfg.horizon;

    let mut visited = FaceSeq::default(); // distinct faces, in order of visit
    let mut t = 0.0;
    let mut orientation = Face::Fore;

    // The ship starts already facing Fore
    if weapon_arcs.iter().enumerate().any(|(i, &arc)| does_arc_mask_cover_face(arc, orientation) && next_ready[i] <= t) {
        visited.push(orientation);
        fire_ready(t, orientation, &weapon_arcs, weapons, &mut events, &mut next_ready);
    }

    // Turn to the next arc to come online, then fire everything in that arc 
    let window = cfg.volley_window.max(0.0);
    let mut turn_time_total = 0.0;
    while let Some((mut face, mut ready)) =
        select_next(candidates, &weapon_arcs, &next_ready, orientation, t, window)
    {

        // If there are ready weapons on both an opposite face and an adjacent face, turn there
        // instead before turning to the opposite face, so the middle arc can be fired as we pass
        // through it
        if ((face != orientation) & (orientation.angle_to(face) >= 180.0)) && 
            let Some(m) =
                intermediate_ready_face(orientation, face, candidates, &weapon_arcs, &next_ready, t)
        {
                face = m;
                ready = weapon_arcs
                    .iter()
                    .enumerate()
                    .filter(|&(_, &arc)| does_arc_mask_cover_face(arc, face))
                    .map(|(w, _)| next_ready[w])
                    .fold(f64::INFINITY, f64::min);
        }

        let mut this_turn = 0.0;
        let arrival = if face != orientation {
            let turn = ship.draw_turn_time(rng, orientation.angle_to(face));
            this_turn = turn;
            orientation = face;
            arrival_time(cfg.turn_timing, t, turn, ready)
        } else {
            t.max(ready)
        };

        t = consolidate_arc_weapons(arrival, face, &weapon_arcs, &next_ready, window);
        if t > horizon {
            break;
        }
        turn_time_total += this_turn;

        visited.push_unique(face);

        fire_ready(t, face, &weapon_arcs, weapons, &mut events, &mut next_ready);
    }

    // Truncate the time of each shot to the sample grid precision, i.e. past deciseconds
    if cfg.t_res > 0.0 {
        for e in &mut events {
            e.time = (e.time / cfg.t_res).floor() * cfg.t_res;
        }
    }
    events.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    let (cumulative, total, peak) = sample(&events, cfg);
    let mut recycle_times: Vec<f64> = weapons.iter().map(|w| w.recycle_time).filter(|t| *t > 0.0).collect();
    recycle_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    recycle_times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    DamageSignal {
        dt: cfg.t_res,
        horizon: cfg.horizon,
        cumulative,
        total,
        peak,
        events,
        rotation: visited.iter().collect(),
        turn_time_total,
        recycle_times,
    }
}

/// Sample damage signal
fn sample(events: &[Fire], cfg: &SimConfig) -> (Vec<(f64, f64)>, f64, f64) {
    let mut curve: Vec<(f64, f64)> = Vec::with_capacity(2 * events.len() + 2);
    curve.push((0.0, 0.0));
    let mut total = 0.0;
    let mut peak = 0.0_f64;

    let mut i = 0;
    while i < events.len() {
        let t = events[i].time;
        let mut bin = 0.0;
        while i < events.len() && events[i].time == t {
            bin += events[i].damage;
            i += 1;
        }
        curve.push((t, total));
        total += bin;
        curve.push((t, total));
        peak = peak.max(bin);
    }
    curve.push((cfg.horizon, total));

    (curve, total, peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_check_correct() {
       let testface: Face = Face::Fore;
       assert_eq!(testface.angle_to(Face::Aft), 180.0);
       assert_eq!(testface.angle_to(Face::Starboard), 90.0);
       assert_eq!(testface.angle_to(Face::Port), 90.0);
       assert_eq!(testface.angle_to(Face::Ventral), 90.0);
       assert_eq!(testface.angle_to(Face::Dorsal), 90.0);
       assert_eq!(testface.angle_to(Face::Fore), 0.0);

       // The face variants cannot be reordered, as we use bitwise arithmetic
       // on the enum discriminants
       assert_eq!(Face::Fore.opposite(), Face::Aft);
       assert_eq!(Face::Aft.opposite(), Face::Fore);
       assert_eq!(Face::Port.opposite(), Face::Starboard);
       assert_eq!(Face::Starboard.opposite(), Face::Port);
       assert_eq!(Face::Dorsal.opposite(), Face::Ventral);
       assert_eq!(Face::Ventral.opposite(), Face::Dorsal);
    }

    #[test]
    fn anticipatory_hides_turn_under_a_long_recycle() {
        assert_eq!(arrival_time(TurnTimingModel::Anticipatory, 0.0, 3.0, 30.0), 30.0);
    }

    #[test]
    fn anticipatory_charges_only_the_unhidden_turn_on_overlap() {
        assert_eq!(arrival_time(TurnTimingModel::Anticipatory, 10.0, 5.0, 12.0), 15.0);
    }

    #[test]
    fn anticipatory_adds_full_turn_when_all_weapons_ready() {
        assert_eq!(arrival_time(TurnTimingModel::Anticipatory, 0.0, 4.0, 0.0), 4.0);
    }

    #[test]
    fn reactive_always_adds_full_turn_after_the_wait() {
        assert_eq!(arrival_time(TurnTimingModel::Reactive, 0.0, 3.0, 30.0), 33.0);
        assert_eq!(arrival_time(TurnTimingModel::Reactive, 10.0, 5.0, 12.0), 17.0);
        assert_eq!(arrival_time(TurnTimingModel::Reactive, 0.0, 4.0, 0.0), 4.0);
    }
}





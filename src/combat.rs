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
pub enum Face {
    Fore,
    Aft,
    Port,
    Starboard,
    Dorsal,
    Ventral,
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

    /// The face on the opposite side of the cube (a 180 deg turn away)
    #[inline]
    pub const fn opposite(self) -> Face {
        match self {
            Face::Fore => Face::Aft,
            Face::Aft => Face::Fore,
            Face::Port => Face::Starboard,
            Face::Starboard => Face::Port,
            Face::Dorsal => Face::Ventral,
            Face::Ventral => Face::Dorsal,
        }
    }

    #[inline]
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

    /// Turn angle, in degrees
    #[inline]
    pub fn angle_to(self, other: Face) -> f64 {
        if self == other {
            0.0
        } else if self.opposite() == other {
            180.0
        } else {
            90.0
        }
    }

    #[inline]
    fn priority(self) -> usize {
        Face::ALL.iter().position(|f| *f == self).unwrap()
    }
}

/// Faces named in a weapon's arc string (e.g. "FAPSD")
fn arc_faces(arc: &str) -> SmallVec<[Face;6]> {
    let mut faces = SmallVec::new();
    for c in arc.chars() {
        if let Some(f) = Face::from_char(c) {
            if !faces.contains(&f) {
                faces.push(f);
            }
        }
    }
    faces
}

/// Per-weapon arcs
fn weapon_faces(weapons: &[Weapon]) -> Vec<SmallVec<[Face; 6]>> {
    weapons.iter().map(|w| arc_faces(&w.arc)).collect()
}

/// The distinct faces any weapon can bear on
fn candidate_faces(faces: &[SmallVec<[Face; 6]>]) -> SmallVec<[Face; 6]> {
    Face::ALL
        .into_iter()
        .filter(|f| faces.iter().any(|fs| fs.contains(f)))
        .collect()
}

/// Choose the next face to present and its earliest bearing weapon's ready time
fn select_next(
    candidates: &[Face],
    faces: &[SmallVec<[Face; 6]>],
    next_ready: &[f64],
    orientation: Face,
    t: f64,
    window: f64,
) -> Option<(Face, f64)> {

    let mut best: Option<(Face, f64)> = None;
    let mut best_key: Option<(f64, isize, f64, usize)> = None;
    for &f in candidates {
        let bearing = || faces.iter().enumerate().filter(|(_, fs)| fs.contains(&f));

        let ready = bearing()
            .map(|(w, _)| next_ready[w])
            .fold(f64::INFINITY, f64::min);

        if !ready.is_finite() {
            // Every weapon in this arc is spent
            continue; 
        }

        // Weapons ready by the earliest feasible fire time (>= now) plus window
        let fire_by = ready.max(t) + window;
        let group = bearing().filter(|(w, _)| next_ready[*w] <= fire_by).count();
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
fn consolidate(
    arrival: f64,
    face: Face,
    faces: &[SmallVec<[Face; 6]>],
    next_ready: &[f64],
    window: f64,
) -> f64 {
    let cutoff = arrival + window;
    let mut fire_t = arrival;
    for (i, fs) in faces.iter().enumerate() {
        if fs.contains(&face) && next_ready[i] <= cutoff && next_ready[i] > fire_t {
            fire_t = next_ready[i];
        }
    }
    fire_t
}

/// The adjacent face (90 deg from current face) with the most ready weapons
fn productive_intermediate(
    orientation: Face,
    target: Face,
    candidates: &[Face],
    faces: &[SmallVec<[Face; 6]>],
    next_ready: &[f64],
    t: f64,
) -> Option<Face> {
    let mut best: Option<(usize, Face)> = None;
    for &m in candidates {
        if orientation.angle_to(m) != 90.0 || m.angle_to(target) != 90.0 {
            continue;
        }
        let exclusive = faces
            .iter()
            .enumerate()
            .filter(|(w, fs)| fs.contains(&m) && !fs.contains(&target) && next_ready[*w] <= t)
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
    /// Sample times 0, dt, 2*dt, ... N*dt, up to horizon
    pub times: Vec<f64>,
    /// cumulative damage dealt by time t
    pub cumulative: Vec<f64>,
    /// damage signal as Dirac impulse train
    pub instantaneous: Vec<f64>,
    /// Every discharge, in time order
    pub events: Vec<Fire>,
    /// The face visit order the ship cycled through
    pub rotation: Vec<Face>,
    /// total seconds spent turning between arcs
    pub turn_time_total: f64,
    /// distinct weapon recycle times, ascending (excludes one-shot weapons)
    pub recycle_times: Vec<f64>,
}

/// Fire every weapon that bears on face and has recycled by time t
fn fire_ready(
    t: f64,
    face: Face,
    faces: &[SmallVec<[Face; 6]>],
    weapons: &[Weapon],
    events: &mut Vec<Fire>,
    next_ready: &mut [f64],
) {
    for (i, fs) in faces.iter().enumerate() {
        if fs.contains(&face) && next_ready[i] <= t {
            events.push(Fire {
                time: t,
                weapon: i,
                face,
                damage: weapons[i].damage,
            });
            next_ready[i] = if weapons[i].recycle_time > 0.0 {
                t + weapons[i].recycle_time
            } else {
                f64::INFINITY // no recycle: fires exactly once
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
    // Resolve any attached character's tunings, cloning only when tuning applies
    let effective;
    let ship = if ship.character.is_some() {
        effective = ship.effective();
        &effective
    } else {
        ship
    };
    let weapons = &ship.weapons;
    let faces = weapon_faces(weapons);
    let candidates = candidate_faces(&faces);

    let mut events: Vec<Fire> = Vec::new();
    let mut next_ready = vec![0.0f64; weapons.len()];
    let horizon = cfg.horizon;

    let mut visited: Vec<Face> = Vec::with_capacity(256); // distinct faces, first-visit order
    let mut t = 0.0;
    let mut orientation = Face::Fore;

    // The ship starts already facing Fore, so anything ready there fires for
    // free before the optimizer gets a chance to turn away from it
    if faces.iter().enumerate().any(|(i, fs)| fs.contains(&orientation) && next_ready[i] <= t) {
        visited.push(orientation);
        fire_ready(t, orientation, &faces, weapons, &mut events, &mut next_ready);
    }

    // Turn to the next arc to come online, then fire everything that bears
    let window = cfg.volley_window.max(0.0);
    let mut turn_time_total = 0.0;
    while let Some((mut face, mut ready)) =
        select_next(&candidates, &faces, &next_ready, orientation, t, window)
    {
        // Reroute a 180 through a perpendicular arc with ready weapons the
        // target can't fire
        if face != orientation && orientation.angle_to(face) >= 180.0 {
            if let Some(m) =
                productive_intermediate(orientation, face, &candidates, &faces, &next_ready, t)
            {
                face = m;
                ready = faces
                    .iter()
                    .enumerate()
                    .filter(|(_, fs)| fs.contains(&face))
                    .map(|(w, _)| next_ready[w])
                    .fold(f64::INFINITY, f64::min);
            }
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
        t = consolidate(arrival, face, &faces, &next_ready, window);
        if t > horizon {
            break;
        }
        turn_time_total += this_turn;
        if !visited.contains(&face) {
            visited.push(face);
        }
        fire_ready(t, face, &faces, weapons, &mut events, &mut next_ready);
    }

    // Truncate the time of each shot to the sample grid precision, i.e. past deciseconds
    if cfg.t_res > 0.0 {
        for e in &mut events {
            e.time = (e.time / cfg.t_res).floor() * cfg.t_res;
        }
    }
    events.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    let (times, cumulative, instantaneous) = sample(&events, cfg);
    let mut recycle_times: Vec<f64> = weapons.iter().map(|w| w.recycle_time).filter(|t| *t > 0.0).collect();
    recycle_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    recycle_times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    DamageSignal {
        dt: cfg.t_res,
        times,
        cumulative,
        instantaneous,
        events,
        rotation: visited,
        turn_time_total,
        recycle_times,
    }
}

/// Sample damage signal
fn sample(events: &[Fire], cfg: &SimConfig) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = if cfg.t_res > 0.0 {
        (cfg.horizon / cfg.t_res).floor() as usize + 1
    } else {
        0
    };
    let times: Vec<f64> = (0..n).map(|k| k as f64 * cfg.t_res).collect();

    // Instantaneous damage, each shot snapped to the nearest sample
    let mut instantaneous = vec![0.0; n];
    for ev in events {
        let k = (ev.time / cfg.t_res).round() as isize;
        if k >= 0 && (k as usize) < n {
            instantaneous[k as usize] += ev.damage;
        }
    }

    // Cumulative: running sum of the damage pulse train 
    let mut cumulative = vec![0.0; n];
    let mut acc = 0.0;
    for k in 0..n {
        acc += instantaneous[k];
        cumulative[k] = acc;
    }

    (times, cumulative, instantaneous)
}

#[cfg(test)]
mod tests {
    use super::*;

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





use crate::character::Character;
use crate::rng::TurnRng;
use crate::weapon::{Weapon, WeaponType};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ship {
    // ID/class/etc. fields
    pub dbref: i64,
    pub name: String,
    pub category: Option<String>,
    pub art: Option<String>, // The ASCII art! :D
    pub class: String,
    pub sensor_class: String,
    pub ship_type: String,

    // Crew stuff (that isn't really relevant anymore?)
    pub crew: f64,
    pub crew_tuned: bool,
    pub quota: i64,
    pub cost: i64,

    // Hull data
    pub structure: f64,
    pub repair: f64,
    pub mass: i64,
    pub bay: i64,
    pub cargo: i64,

    // land/dock flags 
    pub has_land: bool,
    pub has_dock: bool,
    pub can_land: bool,
    pub can_dock: bool,

    // Science con relevant stats
    pub firing: f64,
    pub stealth: f64,
    pub sensors: f64,
    pub cloak_eff: f64, // "Cloak:" on the Sensors line
    pub cloak: Option<f64>, // device strength; None if no device

    // Engineering con relevant stats
    pub fuel_eff: f64,
    pub aux_max: f64,
    pub main_max: f64,
    pub fuel_max: f64,
    pub main: f64,
    pub aux: f64,
    pub batt: f64,
    pub warp_cruise: Option<f64>,
    pub warp_emer: Option<f64>,
    pub warp_max: Option<f64>,
    pub warp_cruise_cost: Option<f64>,
    pub warp_emer_cost: Option<f64>,
    pub warp_max_cost: Option<f64>,
    pub warp_type: Option<String>,
    pub imp_cruise: f64,
    pub imp_emer: f64,
    pub imp_max: f64,
    pub imp_cruise_cost: f64,
    pub imp_emer_cost: f64,
    pub imp_max_cost: f64,


    // Operations con related stats
    // ((Damage input/Shield output) - (Armor Value/100))/(Armor Value/100)
    pub armor: f64,
    pub shield_max: f64,
    pub shield_ratio: f64,
    pub shield_def_1x: f64,
    pub shield_def_2x: f64,
    pub shield_def_3x: f64,
    pub shield_def_4x: f64,
    pub shield_cost_1x: f64,
    pub shield_cost_2x: f64,
    pub shield_cost_3x: f64,
    pub shield_cost_4x: f64,

    // Helm con related stats 
    pub move_ratio: f64,

    // Capability flags
    pub lrs: bool,
    pub srs: bool,
    pub ew: bool,
    pub trans: bool,
    pub tractor: bool,
    pub has_cloak: bool,

    // Weapon systems summary 
    pub beams_count: i64,
    pub beams_dps: f64,
    pub missiles_count: Option<i64>,
    pub missiles_dps: Option<f64>,

    // Weapon list
    pub weapons: Vec<Weapon>,


    // Provenance
    pub source_log: Option<String>,
    pub source_line: Option<i64>,
    pub parsed_at: Option<String>,

    /// None uses the ship's base specs; otherwise applies character tunings
    pub character: Option<Character>,
}

impl Ship {

    #[inline]
    pub fn is_base(&self) -> bool {
        self.ship_type == "Base"
    }

    /// Attach a tuning character, returning the ship for function chaining
    pub fn with_character(mut self, character: Character) -> Self {
        self.character = Some(character);
        self
    }

    /// This ship with its attached character's tunings applied, or a clone of
    /// the base specs if None
    pub fn effective(&self) -> Self {
        match &self.character {
            Some(c) => c.tune_ship(self),
            None => self.clone(),
        }
    }

    /// Weapons of one kind, in slot order
    pub fn weapons_of<'a>(&'a self, kind: &'a WeaponType) -> impl Iterator<Item = &'a Weapon> {
        self.weapons.iter().filter(move |w| &w.weapon_type == kind)
    }

    /// Combined sustained DPS across every weapon
    pub fn total_dps(&self) -> f64 {
        self.weapons.iter().map(|w| w.dps).sum()
    }

    /// Total damage from forward-arc weapons in one volley; always the
    /// most powerful arc afaik
    #[inline]
    pub fn forward_alpha(&self) -> f64 {
        self.weapons
            .iter()
            .filter(|w| w.is_forward())
            .map(|w| w.damage)
            .sum()
    }

    #[inline]
    pub fn turn_rate(&self) -> f64 {
        // In degrees, without noise
        308.6 / (self.move_ratio + 5.38)
    }

    /// ang_deg * (mr + 5.38) / 308.6 plus Gaussian uncertainty 
    /// that was present at the time observations were made 
    pub fn draw_turn_time(&self, rng: &mut TurnRng, ang_deg: f64) -> f64 {

        // Observed SIGMA may be smaller or larger depending on server load, network conditions, etc.
        const SIGMA: f64 = 0.08; 
        let mr = self.move_ratio;
        let noise = rng.next_gaussian(0.0, SIGMA);
        ang_deg * (mr + 5.38) / 308.6 + noise
    }
}


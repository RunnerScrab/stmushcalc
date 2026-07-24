//! Parse @listspecs output out of game logs

use memchr::*;
use std::sync::LazyLock;
use fxhash::FxHashMap;
use smallvec::*;

use regex_lite::Regex;

use crate::ship::Ship;
use crate::weapon::{Weapon, WeaponType};

static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"~~+\s*(?P<inner>.*?)\s*\(#(?P<dbref>\d+)\)\s*~~+\s*$").unwrap());
static BEAMS_HDR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*Beams:\s*(\d+)\s+DPS:\s*([\d.]+)").unwrap());
static MISSILES_HDR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*Missiles:\s*(\d+)\s+DPS:\s*([\d.]+)").unwrap());
static WEAPON_ROW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(Cost|Range|Arcs|Damage|Time|DPS):\s+(.*\S)\s*$").unwrap());
static ART_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(Class|SensorClass|Type|Structure):").unwrap());

mod label_type {
    pub(crate) const ONEXCOST: usize = 0;
    pub(crate) const BAY: usize = 1;
    pub(crate) const CLASS: usize = 2;
    pub(crate) const CLOAK: usize = 3;
    pub(crate) const COST: usize = 4;
    pub(crate) const DEFENSE: usize = 5;
    pub(crate) const FIRING: usize = 6;
    pub(crate) const HASLAND: usize = 7;
    pub(crate) const IMPCRUISE: usize = 8;
    pub(crate) const LRS: usize = 9;
    pub(crate) const MAIN: usize = 10;
    pub(crate) const MAINMAX: usize = 11;
    pub(crate) const MOVERATIO: usize = 12;
    pub(crate) const SENSORCLASS: usize = 13;
    pub(crate) const SHIELDMAX: usize = 14;
    pub(crate) const STRUCTURE: usize = 15;
    pub(crate) const TRANS: usize = 16;
    pub(crate) const TYPE: usize = 17;
    pub(crate) const WARPTYPE: usize = 18;
    pub(crate) const WARPCRUISE: usize = 19;
}

static SPEC_LABELS: [&str; 20] = [ 
     "1x Cost",
     "Bay",
     "Class",
     "Cloak",
     "Cost",
     "Defense",
     "Firing",
     "HasLand",
     "ImpCruise",
     "LRS",
     "Main",
     "MainMax",
     "MoveRatio",
     "SensorClass",
     "ShieldMax",
     "Structure",
     "Trans",
     "Type",
     "Warp Type",
     "WarpCruise",
];

#[derive(Clone, Copy)]
enum ExpectCost {
    Warp,
    Imp,
}

#[derive(Default)]
struct ShipSpecBlock {
    dbref: i64,
    name: String,
    category: Option<String>,

    class: Option<String>,
    sensor_class: Option<String>,
    ship_type: Option<String>,
    crew: Option<f64>,
    crew_tuned: bool,
    quota: Option<i64>,
    cost: Option<i64>,

    structure: Option<f64>,
    repair: Option<f64>,
    mass: Option<i64>,
    bay: Option<i64>,
    cargo: Option<i64>,

    has_land: Option<bool>,
    has_dock: Option<bool>,
    can_land: Option<bool>,
    can_dock: Option<bool>,

    firing: Option<f64>,
    fuel_eff: Option<f64>,
    stealth: Option<f64>,
    cloak_eff: Option<f64>,
    sensors: Option<f64>,
    aux_max: Option<f64>,
    main_max: Option<f64>,
    armor: Option<f64>,
    fuel_max: Option<f64>,

    lrs: Option<bool>,
    srs: Option<bool>,
    ew: Option<bool>,
    trans: Option<bool>,
    tractor: Option<bool>,
    has_cloak: Option<bool>,
    cloak: Option<f64>,

    main: Option<f64>,
    aux: Option<f64>,
    batt: Option<f64>,
    move_ratio: Option<f64>,

    warp_cruise: Option<f64>,
    warp_emer: Option<f64>,
    warp_max: Option<f64>,
    warp_cruise_cost: Option<f64>,
    warp_emer_cost: Option<f64>,
    warp_max_cost: Option<f64>,
    warp_type: Option<String>,

    imp_cruise: Option<f64>,
    imp_emer: Option<f64>,
    imp_max: Option<f64>,
    imp_cruise_cost: Option<f64>,
    imp_emer_cost: Option<f64>,
    imp_max_cost: Option<f64>,

    shield_max: Option<f64>,
    shield_ratio: Option<f64>,
    shield_def: [Option<f64>; 4],
    shield_cost: [Option<f64>; 4],

    beams_count: Option<i64>,
    beams_dps: Option<f64>,
    missiles_count: Option<i64>,
    missiles_dps: Option<f64>,

    weapons: Vec<Weapon>,
    cur_weapon: Option<WeaponType>,
    weapbuf: FxHashMap<String, Vec<String>>,
    expect_cost: Option<ExpectCost>,
    complete: bool,
    art_lines: Vec<String>,
    has_art: bool,
}

impl ShipSpecBlock {
    fn new(dbref: i64, name: String, category: Option<String>) -> Self {
        ShipSpecBlock { dbref, name, category, has_art: true, ..Default::default() }
    }

    fn flush_weapon_section(&mut self) {
        let Some(kind) = self.cur_weapon.take() else {
            self.weapbuf.clear();
            return;
        };
        let arcs = self.weapbuf.get("Arcs").cloned().unwrap_or_default();
        let n = arcs.len();
        if n == 0 {
            self.weapbuf.clear();
            return;
        }

        let weapcol = |label: &str| -> Vec<Option<f64>> {
            let mut v: Vec<Option<f64>> = self
                .weapbuf
                .get(label)
                .map(|xs| xs.iter().map(|s| s.parse::<f64>().ok()).collect())
                .unwrap_or_default();
            v.resize(n, None);
            v
        };

        let (costs, ranges) = (weapcol("Cost"), weapcol("Range"));
        let (dmgs, times, dpss) = (weapcol("Damage"), weapcol("Time"), weapcol("DPS"));

        for i in 0..n {
            self.weapons.push(Weapon {
                weapon_type: kind.clone(),
                slot: i as i64 + 1,
                cost: costs[i].unwrap_or(0.0),
                range: ranges[i].unwrap_or(0.0),
                arc: arcs[i].clone(),
                damage: dmgs[i].unwrap_or(0.0),
                recycle_time: times[i].unwrap_or(0.0),
                dps: dpss[i].unwrap_or(0.0),
            });
        }

        if matches!(kind, WeaponType::Missile) {
            self.complete = true;
        }

        self.weapbuf.clear();
    }

    fn finalize(&mut self) {
        while self.art_lines.first().is_some_and(|l| l.trim().is_empty()) {
            self.art_lines.remove(0);
        }
        while self.art_lines.last().is_some_and(|l| l.trim().is_empty()) {
            self.art_lines.pop();
        }
    }

    fn parse_line(&mut self, line: &str) {
        
        if self.has_art {
            let s = line.trim();
            if !s.is_empty() && s.chars().all(|c| c == '~') {
                self.has_art = false;
                return;
            }
            if ART_END_RE.is_match(line) {
                self.has_art = false;
                // fall through: parse this line as a field
            } else {
                self.art_lines.push(line.trim_end().to_string());
                return;
            }
        }

        if let Some(m) = BEAMS_HDR_RE.captures(line) {
            self.flush_weapon_section();
            self.beams_count = m[1].parse().ok();
            self.beams_dps = m[2].parse().ok();
            self.cur_weapon = Some(WeaponType::Beam);
            self.weapbuf.clear();
            return;
        }
        if let Some(m) = MISSILES_HDR_RE.captures(line) {
            self.flush_weapon_section();
            self.missiles_count = m[1].parse().ok();
            self.missiles_dps = m[2].parse().ok();
            self.cur_weapon = Some(WeaponType::Missile);
            self.weapbuf.clear();
            return;
        }

        if let Some(kind) = self.cur_weapon.clone() {
            if let Some(m) = WEAPON_ROW_RE.captures(line) {
                let (label, rest) = (m[1].to_string(), &m[2]);
                let toks: Vec<String> = if label == "Arcs" {
                    // Weapon arcs
                    let b = rest.as_bytes();
                    (0..b.len())
                        .step_by(6)
                        .map(|i| rest[i..(i + 6).min(rest.len())].trim().to_string())
                        .collect()
                } else {
                    rest.split_whitespace().map(str::to_string).collect()
                };
                self.weapbuf.entry(label.clone()).or_default().extend(toks);
                if label == "DPS" {
                    let declared = match kind {
                        WeaponType::Missile => self.missiles_count,
                        WeaponType::Beam => self.beams_count,
                        WeaponType::Other(_) => None,
                    };
                    let have = self.weapbuf.get("Arcs").map_or(0, Vec::len) as i64;
                    if declared.is_none_or(|d| have >= d) {
                        self.flush_weapon_section();
                    }
                }
                return;
            }
            if line.trim_matches(|c: char| c == '~' || c.is_whitespace()).is_empty() {
                return;
            }
        }

        let label = memchr(b':', line.as_bytes()).map_or("", |i| line[..i].trim());
        
        if let Ok(idx) = &SPEC_LABELS.binary_search(&label) {
            match *idx {
                label_type::CLASS => {
                    self.class = string_after_label(line, "Class");
                    self.crew = float_after_label(line, "Crew");
                    self.crew_tuned = line.contains("(tuned)");
                }
                label_type::SENSORCLASS => {
                    self.sensor_class = string_after_label(line, "SensorClass");
                    self.quota = int_after_label(line, "Quota");
                }
                label_type::TYPE => {
                    self.ship_type = string_after_label(line, "Type")
                        .and_then(|s| s.split_whitespace().next().map(str::to_string));
                    self.cost = int_after_label(line, "Cost");
                }
                label_type::STRUCTURE => {
                    self.structure = float_after_label(line, "Structure");
                    self.repair = float_after_label(line, "Repair");
                    self.mass = int_after_label(line, "Mass");
                }
                label_type::BAY => {
                    self.bay = int_after_label(line, "Bay");
                    self.cargo = int_after_label(line, "Cargo");
                }
                label_type::HASLAND => {
                    self.has_land = bool_after_label(line, "HasLand");
                    self.has_dock = bool_after_label(line, "HasDock");
                    self.can_land = bool_after_label(line, "CanLand");
                    self.can_dock = bool_after_label(line, "CanDock");
                }
                label_type::FIRING => {
                    self.firing = float_after_label(line, "Firing");
                    self.fuel_eff = float_after_label(line, "FuelEff");
                    self.stealth = float_after_label(line, "Stealth");
                }
                label_type::CLOAK => {
                    self.cloak_eff = float_after_label(line, "Cloak");
                    self.sensors = float_after_label(line, "Sensors");
                    self.aux_max = float_after_label(line, "AuxMax");
                }
                label_type::MAINMAX => {
                    self.main_max = float_after_label(line, "MainMax");
                    self.armor = float_after_label(line, "Armor");
                    self.fuel_max = float_after_label(line, "FuelMax");
                }
                label_type::LRS => {
                    self.lrs = bool_after_label(line, "LRS");
                    self.srs = bool_after_label(line, "SRS");
                    self.ew = bool_after_label(line, "EW");
                }
                label_type::TRANS => {
                    self.trans = bool_after_label(line, "Trans");
                    self.tractor = bool_after_label(line, "Tractor");

                    // Cloak is either "No" or the cloaking power cost
                    if regex_cloak_no(line) {
                        self.has_cloak = Some(false);
                        self.cloak = None;
                    } else if let Some(cv) = float_after_label(line, "Cloak") {
                        self.has_cloak = Some(true);
                        self.cloak = Some(cv);
                    }
                }
                label_type::MAIN => {
                    self.main = float_after_label(line, "Main");
                    self.aux = float_after_label(line, "Aux");
                    self.batt = float_after_label(line, "Batt");
                }
                label_type::MOVERATIO => self.move_ratio = float_after_label(line, "MoveRatio"),
                label_type::WARPCRUISE => {
                    self.warp_cruise = float_after_label(line, "WarpCruise");
                    self.warp_emer = float_after_label(line, "WarpEmer");
                    self.warp_max = float_after_label(line, "WarpMax");
                    self.expect_cost = Some(ExpectCost::Warp);
                }
                label_type::IMPCRUISE => {
                    self.imp_cruise = float_after_label(line, "ImpCruise");
                    self.imp_emer = float_after_label(line, "ImpEmer");
                    self.imp_max = float_after_label(line, "ImpMax");
                    self.expect_cost = Some(ExpectCost::Imp);
                }
                label_type::WARPTYPE => self.warp_type = string_after_label(line, "Warp Type"),
                label_type::COST if self.expect_cost.is_some() && line.matches("Cost:").count() >= 3 => {
                    let costs = all_floats_after_label(line, "Cost");
                    let g = |i: usize| costs.get(i).copied();
                    match self.expect_cost {
                        Some(ExpectCost::Warp) => {
                            self.warp_cruise_cost = g(0);
                            self.warp_emer_cost = g(1);
                            self.warp_max_cost = g(2);
                        }
                        _ => {
                            self.imp_cruise_cost = g(0);
                            self.imp_emer_cost = g(1);
                            self.imp_max_cost = g(2);
                        }
                    }
                    self.expect_cost = None;
                }
                label_type::SHIELDMAX => {
                    self.shield_max = float_after_label(line, "ShieldMax");
                    self.shield_ratio = float_after_label(line, "ShieldRatio");
                }
                label_type::ONEXCOST => {
                    let v = all_floats_after_label(line, "x Cost");
                    for i in 0..4 {
                        self.shield_cost[i] = v.get(i).copied();
                    }
                }
                label_type::DEFENSE => {
                    let v = all_floats_after_label(line, "Defense");
                    for i in 0..4 {
                        self.shield_def[i] = v.get(i).copied();
                    }
                }
                _ => {
                    // We would not be inside the Ok() block if this we did not
                    // find the label
                    unreachable!();
                }
            }
        }
    }

    fn into_ship(mut self) -> Ship {
        self.finalize();
        let art = if self.art_lines.is_empty() {
            None
        } else {
            Some(self.art_lines.join("\n"))
        };

        // Lots and lots of specs
        Ship {
            dbref: self.dbref,
            name: self.name,
            category: self.category,
            art,
            class: self.class.unwrap_or_default(),
            sensor_class: self.sensor_class.unwrap_or_default(),
            ship_type: self.ship_type.unwrap_or_default(),
            crew: self.crew.unwrap_or(0.0),
            crew_tuned: self.crew_tuned,
            quota: self.quota.unwrap_or(0),
            cost: self.cost.unwrap_or(0),

            structure: self.structure.unwrap_or(0.0),
            repair: self.repair.unwrap_or(0.0),
            mass: self.mass.unwrap_or(0),
            bay: self.bay.unwrap_or(0),
            cargo: self.cargo.unwrap_or(0),

            has_land: self.has_land.unwrap_or(false),
            has_dock: self.has_dock.unwrap_or(false),
            can_land: self.can_land.unwrap_or(false),
            can_dock: self.can_dock.unwrap_or(false),

            firing: self.firing.unwrap_or(0.0),
            fuel_eff: self.fuel_eff.unwrap_or(0.0),
            stealth: self.stealth.unwrap_or(0.0),
            cloak_eff: self.cloak_eff.unwrap_or(0.0),
            sensors: self.sensors.unwrap_or(0.0),
            aux_max: self.aux_max.unwrap_or(0.0),
            main_max: self.main_max.unwrap_or(0.0),
            armor: self.armor.unwrap_or(0.0),
            fuel_max: self.fuel_max.unwrap_or(0.0),

            lrs: self.lrs.unwrap_or(false),
            srs: self.srs.unwrap_or(false),
            ew: self.ew.unwrap_or(false),
            trans: self.trans.unwrap_or(false),
            tractor: self.tractor.unwrap_or(false),
            has_cloak: self.has_cloak.unwrap_or(false),
            cloak: self.cloak,

            main: self.main.unwrap_or(0.0),
            aux: self.aux.unwrap_or(0.0),
            batt: self.batt.unwrap_or(0.0),
            move_ratio: self.move_ratio.unwrap_or(0.0),

            warp_cruise: self.warp_cruise,
            warp_emer: self.warp_emer,
            warp_max: self.warp_max,
            warp_cruise_cost: self.warp_cruise_cost,
            warp_emer_cost: self.warp_emer_cost,
            warp_max_cost: self.warp_max_cost,
            warp_type: self.warp_type,

            imp_cruise: self.imp_cruise.unwrap_or(0.0),
            imp_emer: self.imp_emer.unwrap_or(0.0),
            imp_max: self.imp_max.unwrap_or(0.0),
            imp_cruise_cost: self.imp_cruise_cost.unwrap_or(0.0),
            imp_emer_cost: self.imp_emer_cost.unwrap_or(0.0),
            imp_max_cost: self.imp_max_cost.unwrap_or(0.0),

            shield_max: self.shield_max.unwrap_or(0.0),
            shield_ratio: self.shield_ratio.unwrap_or(0.0),
            shield_def_1x: self.shield_def[0].unwrap_or(0.0),
            shield_def_2x: self.shield_def[1].unwrap_or(0.0),
            shield_def_3x: self.shield_def[2].unwrap_or(0.0),
            shield_def_4x: self.shield_def[3].unwrap_or(0.0),
            shield_cost_1x: self.shield_cost[0].unwrap_or(0.0),
            shield_cost_2x: self.shield_cost[1].unwrap_or(0.0),
            shield_cost_3x: self.shield_cost[2].unwrap_or(0.0),
            shield_cost_4x: self.shield_cost[3].unwrap_or(0.0),

            beams_count: self.beams_count.unwrap_or(0),
            beams_dps: self.beams_dps.unwrap_or(0.0),
            missiles_count: self.missiles_count,
            missiles_dps: self.missiles_dps,

            source_log: Some("upload".to_string()),
            source_line: None,
            parsed_at: None,

            weapons: self.weapons,
            character: None,
        }
    }

    fn is_valid(&self) -> bool {
        self.structure.is_some()
            && self.beams_count.is_some()
            && self.category.is_some() 
    }
}

fn regex_cloak_no(line: &str) -> bool {
    if let Some(i) = memmem::find(line.as_bytes(), "Cloak:".as_bytes()) {
        let rest = line[i + "Cloak:".len()..].trim_start();
        return rest.starts_with("No") && !rest[2..].starts_with(|c: char| c.is_alphanumeric());
    }
    false
}

fn title_parts(inner: &str) -> (Option<String>, String) {
    if inner.ends_with(')') {
        if let Some(open) = memrchr(b'(', inner.as_bytes()) {
            let cat = &inner[open + 1..inner.len() - 1];
            if memchr(b'(', cat.as_bytes()).is_none() && memrchr(b')', cat.as_bytes()).is_none() { 
                return (Some(cat.trim().to_string()), inner[..open].trim().to_string());
            }
        }
    }
    (None, inner.trim().to_string())
}

pub fn parse_logs(text: &[u8]) -> Vec<Ship> {
    let mut ships = Vec::with_capacity(64);
    let mut block: Option<ShipSpecBlock> = None;

    let finish_ship = |b: Option<ShipSpecBlock>, out: &mut Vec<Ship>| {
        if let Some(mut b) = b {
            b.flush_weapon_section();
            if b.is_valid() {
                out.push(b.into_ship());
            }
        }
    };

    // Break the log file text into lines
    for line in get_lines_memchr(text) {
        if line.trim_start().starts_with('~') {
            if let Some(m) = TITLE_RE.captures(line.trim()) {
                finish_ship(block.take(), &mut ships);
                let inner = m.name("inner").unwrap().as_str();
                let dbref: i64 = m.name("dbref").unwrap().as_str().parse().unwrap_or(0);
                let (cat, name) = title_parts(inner);
                block = Some(ShipSpecBlock::new(dbref, name, cat));
                continue;
            }
        }
        match &mut block {
            Some(b) if !b.complete => b.parse_line(line),
            _ => {}
        }
    }
    finish_ship(block.take(), &mut ships);
    ships
}

pub fn get_lines_memchr(buffer: &[u8]) -> impl Iterator<Item = &str> + '_ {
    let mut pos: usize = 0;

    std::iter::from_fn(move || {
        if pos >= buffer.len() {
            return None;
        }

        let remainder = &buffer[pos..];
        match memchr2(b'\r', b'\n', remainder) {
            Some(found) => {
                let line = std::str::from_utf8(&remainder[..found]).unwrap_or("");
                let line_end = pos + found;

                pos += found + 1;
                
                let has_lf = ((buffer[line_end] == b'\r') as usize)
                    & (pos < buffer.len() && (buffer[pos] == b'\n')) as usize;

                pos += has_lf;

                Some(line)
            }
            None => {
                //The last portion of the buffer did not end with a newline
                let line = std::str::from_utf8(remainder).unwrap_or("");
                pos = buffer.len();
                Some(line)
            }
        }
    })
}

/// The float after the first "label:" on the line
fn float_after_label(line: &str, label: &str) -> Option<f64> {
    let key = format!("{label}:");
    let mut from = 0;

    while let Some(rel) = memmem::find(line[from..].as_bytes(), key.as_bytes()) {
        let rest = line[from + rel + key.len()..].trim_start();
        let tok: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() | (*c == '.') | (*c == '-'))
            .collect();
        if let Ok(v) = tok.trim_end_matches('.').parse::<f64>() {
            return Some(v);
        }
        from += rel + key.len();
    }
    None
}

/// Every float following an occurrence of "label:" on the line, in order
fn all_floats_after_label(line: &str, label: &str) -> SmallVec<[f64; 12]> {
    let key = format!("{label}:");
    let mut out = SmallVec::<[f64; 12]>::new();
    let mut from = 0;

    while let Some(rel) = memmem::find(line[from..].as_bytes(), key.as_bytes()) {
        from += rel + key.len();
        let rest = line[from..].trim_start();
        let tok: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() | (*c == '.') | (*c == '-'))
            .collect();
        if let Ok(v) = tok.trim_end_matches('.').parse::<f64>() {
            out.push(v);
        }
    }
    out
}

#[inline(always)]
fn int_after_label(line: &str, label: &str) -> Option<i64> {
    float_after_label(line, label).map(|v| v as i64)
}

fn string_after_label(line: &str, label: &str) -> Option<String> {
    let key = format!("{label}:");
    let idx = memmem::find(line.as_bytes(), key.as_bytes())?;
    let rest = line[idx + key.len()..].trim_start();
    let bytes = rest.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        if let Some(pos) = memchr(b' ', &bytes[i..]) {
            i += pos;
            
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            
            if i - start >= 2 {
                let next = rest[i..].split_whitespace().next().unwrap_or("");
                if next.contains(':') {
                    return Some(rest[..start].trim().to_string());
                }
            }
        } else {
            break;
        }
    }
    Some(rest.trim().to_string())
}

fn bool_after_label(line: &str, label: &str) -> Option<bool> {
    let key = format!("{label}:");
    let rest = line[memmem::find(line.as_bytes(), key.as_bytes())? + key.len()..].trim_start();
    if rest.starts_with("Yes") {
        Some(true)
    } else if rest.starts_with("No") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFLEX: &str = "\
~~~~~~~~~~~~ UFP Reflex C (Reflex) (#5367) ~~~~~~~~~~~~
       Class: UFP Reflex C                      Crew: 7
 SensorClass: Reflex                           Quota: 7
        Type: Ship                              Cost: 359860900
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
  Structure: 737               Repair: 737                 Mass: 3200000
        Bay: 30000              Cargo: 25
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    HasLand: Yes      HasDock: No       CanLand: No       CanDock: Yes
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
     Firing: 100              FuelEff: 100              Stealth: 100
      Cloak: 100              Sensors: 100               AuxMax: 100
    MainMax: 100                Armor: 100              FuelMax: 1000
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        LRS: Yes                  SRS: Yes                   EW: Yes
      Trans: Yes              Tractor: Yes                Cloak: No
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
       Main: 498                  Aux: 29                  Batt: 29
  MoveRatio: 8
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
 WarpCruise: 14.1            WarpEmer: 16.2             WarpMax: 18.3
       Cost: 159.048             Cost: 209.952             Cost: 267.912
  Warp Type: Standard
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
  ImpCruise: 0.25             ImpEmer: 0.50              ImpMax: 0.75
       Cost: 9.6                 Cost: 14.4                Cost: 28.8
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
  ShieldMax: 24           ShieldRatio: 4
 1x Cost: 6          2x Cost: 12         3x Cost: 18         4x Cost: 24
 Defense: 95.8333    Defense: 97.2222    Defense: 97.619     Defense: 97.7778
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
       Beams: 14                                 DPS: 192.9
        Cost: 1695  1695  341   341   341   341   341   341   341   341
       Range: 5000  5000  4800  4800  4800  4800  4800  4800  4800  4800
        Arcs: F     F     FD    FV    FPD   FPV   FSD   FSV   APD   APV
      Damage: 1695  1695  341   341   341   341   341   341   341   341
        Time: 60.0  60.0  30.0  30.0  30.0  30.0  30.0  30.0  30.0  30.0
         DPS: 28.2  28.2  11.4  11.4  11.4  11.4  11.4  11.4  11.4  11.4
        Cost: 341   341   341   341
       Range: 4800  4800  4800  4800
        Arcs: ASD   ASV   AD    AV
      Damage: 341   341   341   341
        Time: 30.0  30.0  30.0  30.0
         DPS: 11.4  11.4  11.4  11.4
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    Missiles: 12                                 DPS: 160.0
        Cost: 1     1     1     1     1     1     1     1     1     1
       Range: 3200  3200  3200  3200  3200  3200  3200  3200  3200  3200
        Arcs: F     F     F     F     F     F     A     A     A     A
      Damage: 1200  1200  1200  1200  1200  1200  1200  1200  1200  1200
        Time: 90.0  90.0  90.0  90.0  90.0  90.0  90.0  90.0  90.0  90.0
         DPS: 13.3  13.3  13.3  13.3  13.3  13.3  13.3  13.3  13.3  13.3
        Cost: 1     1
       Range: 3200  3200
        Arcs: A     A
      Damage: 1200  1200
        Time: 90.0  90.0
         DPS: 13.3  13.3
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
";

    #[test]
    fn parses_specs_correctly() {
        let ships = parse_logs(REFLEX.as_bytes());
        assert_eq!(ships.len(), 1);
        let s = &ships[0];
        assert_eq!(s.dbref, 5367);
        assert_eq!(s.name, "UFP Reflex C");
        assert_eq!(s.category.as_deref(), Some("Reflex"));
        assert_eq!(s.crew, 7.0);
        assert_eq!(s.structure, 737.0);
        assert_eq!(s.shield_max, 24.0);
        assert_eq!(s.move_ratio, 8.0);
        assert_eq!(s.beams_count, 14);
        assert_eq!(s.missiles_count, Some(12));
        assert_eq!(s.has_cloak, false);
        assert_eq!(s.warp_type.as_deref(), Some("Standard"));

        let beams: Vec<_> = s.weapons.iter().filter(|w| matches!(w.weapon_type, WeaponType::Beam)).collect();
        let missiles: Vec<_> = s.weapons.iter().filter(|w| matches!(w.weapon_type, WeaponType::Missile)).collect();
        assert_eq!(beams.len(), 14);
        assert_eq!(missiles.len(), 12);
        assert_eq!(beams[0].cost, 1695.0);
        assert_eq!(beams[0].arc, "F");
        assert_eq!(beams[2].arc, "FD");
        assert_eq!(missiles[6].arc, "A");
        assert_eq!(missiles[0].damage, 1200.0);
    }
}


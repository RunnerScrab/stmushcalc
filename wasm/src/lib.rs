use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use plotters::coord::Shift;
use plotters::prelude::{DrawingArea, IntoDrawingArea, RGBColor};
use plotters::backend::SVGBackend;
use shipdb::plot::{
    render_cumulative as plot_cumulative, render_instantaneous as plot_instantaneous, Theme, DARK,
};
use shipdb::{
    is_transwarp, simulate_damage, Character, DamageSignal, Ship, SimConfig, TurnRng,
    TurnTimingModel, WeaponType, DEFAULT_HORIZON_SECS as HORIZON_SECS, DEFAULT_RNG_SEED as RNG_SEED,
    DEFAULT_SAMPLE_DT as SAMPLE_DT, DEFAULT_VOLLEY_WINDOW as VOLLEY_WINDOW,
};
use wasm_bindgen::prelude::*;

const SHIPS_BIN: &[u8] = include_bytes!("../../ships.bin");

// the ASCII art is in a side table keyed by dbref
fn catalog() -> &'static (Vec<Ship>, HashMap<i64, String>) {
    static CAT: OnceLock<(Vec<Ship>, HashMap<i64, String>)> = OnceLock::new();
    CAT.get_or_init(|| {
        let mut ships = shipdb::ships_from_bytes(SHIPS_BIN).expect("embedded ships.bin");
        let art: HashMap<i64, String> = ships
            .iter_mut()
            .filter_map(|s| s.art.take().map(|a| (s.dbref, a)))
            .collect();
        (ships, art)
    })
}

#[inline]
fn ships() -> &'static [Ship] {
    &catalog().0
}

#[inline]
fn ship_names_lower() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| ships().iter().map(|s| s.name.to_lowercase()).collect())
}

thread_local! {
    // Ships scraped from user-uploaded logs this session. Art is stored in a side table
    static UPLOADED: RefCell<Vec<&'static Ship>> = const { RefCell::new(Vec::new()) };
    static UPLOADED_ART: RefCell<HashMap<i64, String>> = RefCell::new(HashMap::new());
}

#[inline]
fn find_ship(name: &str) -> Option<&'static Ship> {
    let needle = name.to_lowercase();
    if let Some(ship) = ships()
        .iter()
        .zip(ship_names_lower())
        .find(|(_, lower)| lower.contains(&needle))
        .map(|(ship, _)| ship)
    {
        return Some(ship);
    }
    UPLOADED.with(|u| {
        u.borrow()
            .iter()
            .find(|s| s.name.to_lowercase().contains(&needle))
            .copied()
    })
}

#[inline]
fn find_ship_or_err(name: &str) -> Result<&'static Ship, JsValue> {
    find_ship(name).ok_or_else(|| jerr(format!("no ship matching {name:?}")))
}

#[inline]
fn jerr(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[derive(PartialEq)]
struct SimParams {
    eng: f64,
    tac: f64,
    helm: f64,
    oper: f64,
    sci: f64,
    dam: f64,
    wis: f64,
    timing: TurnTimingModel,
}

thread_local! {
    static SIM_CACHE: RefCell<Option<(SimParams, HashMap<i64, Rc<DamageSignal>>)>> = const { RefCell::new(None) };
}

#[cfg(test)]
static SIM_RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn run(ship: &Ship, character: Character, timing: TurnTimingModel) -> Rc<DamageSignal> {
    let params = SimParams {
        eng: character.engineering,
        tac: character.tactical,
        helm: character.helm,
        oper: character.operations,
        sci: character.science,
        dam: character.damage_control,
        wis: character.wisdom,
        timing,
    };
    SIM_CACHE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let fresh = matches!(slot.as_ref(), Some((p, _)) if *p == params);
        if !fresh {
            *slot = Some((params, HashMap::new()));
        }
        let map = &mut slot.as_mut().unwrap().1;
        if let Some(sig) = map.get(&ship.dbref) {
            return Rc::clone(sig);
        }
        let tuned = ship.clone().with_character(character);
        let mut rng = TurnRng::new(RNG_SEED);
        let cfg = SimConfig::new(HORIZON_SECS, SAMPLE_DT, timing, VOLLEY_WINDOW);
        #[cfg(test)]
        SIM_RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let sig = Rc::new(simulate_damage(&tuned, &mut rng, &cfg));
        map.insert(ship.dbref, Rc::clone(&sig));
        sig
    })
}

fn parse_timing(s: &str) -> TurnTimingModel {
    if s.eq_ignore_ascii_case("anticipatory") {
        TurnTimingModel::Anticipatory
    } else {
        TurnTimingModel::Reactive
    }
}

fn theme_with_bg(bg: &str) -> Theme {
    let rgb: Vec<u8> = bg
        .trim_start_matches("rgba(")
        .trim_start_matches("rgb(")
        .trim_end_matches(')')
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    let bg = match rgb.as_slice() {
        [r, g, b, ..] => RGBColor(*r, *g, *b),
        _ => DARK.bg,
    };
    Theme { bg, ..DARK }
}

fn to_svg(
    width: u32,
    height: u32,
    draw: impl FnOnce(&DrawingArea<SVGBackend, Shift>) -> Result<(), JsValue>,
) -> Result<String, JsValue> {
    let mut svg = String::new();
    {
        let root = SVGBackend::with_string(&mut svg, (width, height)).into_drawing_area();
        draw(&root)?;
        root.present().map_err(jerr)?;
    }
    Ok(svg)
}

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn list_ships() -> Vec<String> {
    let mut names: Vec<String> = ships().iter().map(|s| s.name.clone()).collect();
    UPLOADED.with(|u| names.extend(u.borrow().iter().map(|s| s.name.clone())));
    names.sort_by_key(|n| n.to_lowercase());
    names
}

#[wasm_bindgen]
pub fn ship_art(name: &str) -> Result<String, JsValue> {
    let ship = find_ship_or_err(name)?;
    if let Some(a) = catalog().1.get(&ship.dbref) {
        return Ok(a.clone());
    }
    Ok(UPLOADED_ART.with(|m| m.borrow().get(&ship.dbref).cloned()).unwrap_or_default())
}

fn add_ships(incoming: impl IntoIterator<Item = Ship>) -> Vec<String> {
    let mut added = Vec::new();
    for mut s in incoming {
        // the embedded catalog is immutable, so it wins on dbref collisions
        if ships().iter().any(|e| e.dbref == s.dbref) {
            continue;
        }
        let dbref = s.dbref;
        if let Some(a) = s.art.take() {
            UPLOADED_ART.with(|m| m.borrow_mut().insert(dbref, a));
        }
        let leaked: &'static Ship = Box::leak(Box::new(s));
        UPLOADED.with(|u| {
            let mut u = u.borrow_mut();
            match u.iter_mut().find(|e| e.dbref == dbref) {
                Some(slot) => *slot = leaked, // replace a re-uploaded ship
                None => u.push(leaked),
            }
        });
        added.push(leaked.name.clone());
    }
    added
}

/// Scrape @listspecs output from loaded logs and cache them locally
#[wasm_bindgen]
pub fn add_ships_from_log(text: &str) -> Vec<String> {
    add_ships(shipdb::logparse::parse_logs(text))
}

/// Bincode snapshot of this session's uploaded ships, art re-attached
#[wasm_bindgen]
pub fn export_uploaded() -> Vec<u8> {
    let ships: Vec<Ship> = UPLOADED.with(|u| {
        u.borrow()
            .iter()
            .map(|&s| {
                let mut ship = s.clone();
                ship.art = UPLOADED_ART.with(|m| m.borrow().get(&s.dbref).cloned());
                ship
            })
            .collect()
    });
    shipdb::ships_to_bytes(&ships).unwrap_or_default()
}

/// Re-add ships from a prior export_uploaded blob. Returns the names added
#[wasm_bindgen]
pub fn import_uploaded(bytes: &[u8]) -> Vec<String> {
    shipdb::ships_from_bytes(bytes).map(add_ships).unwrap_or_default()
}

/// Returns ships loaded from logs this session
#[wasm_bindgen]
pub fn ship_counts() -> Vec<u32> {
    let uploaded = UPLOADED.with(|u| u.borrow().len()) as u32;
    vec![ships().len() as u32, uploaded]
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn simulate_summary(
    name: &str,
    eng: f64,
    tac: f64,
    helm: f64,
    oper: f64,
    sci: f64,
    dam: f64,
    wis: f64,
    timing: &str,
) -> Result<String, JsValue> {
    let ship = find_ship_or_err(name)?;
    let ch = Character::new(eng, tac, helm, oper, sci, dam, wis);
    let sig = run(ship, ch, parse_timing(timing));
    let rotation: String = sig.rotation.iter().map(|f| f.label()).collect();
    let total = sig.total;
    let dps = total / HORIZON_SECS;
    let turn = sig.turn_time_total;
    Ok(format!(
        "Over {HORIZON_SECS:.0} s sim: {shots} shots, \
         <span data-stat=\"total_damage\" data-val=\"{total}\">{total:.0}</span> damage, \
         <span data-stat=\"sustained_dps\" data-val=\"{dps}\">{dps:.1}</span> simmed DPS, \
         Time turning: <span data-stat=\"turn_time\" data-val=\"{turn}\" data-invert=\"1\">{turn:.1}s</span>, \
         arc rotation [{rotation}]",
        shots = sig.events.len(),
    ))
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn ship_table(
    name: &str,
    eng: f64,
    tac: f64,
    helm: f64,
    oper: f64,
    sci: f64,
    dam: f64,
    wis: f64,
) -> Result<String, JsValue> {
    let ship = find_ship_or_err(name)?;
    let ch = Character::new(eng, tac, helm, oper, sci, dam, wis);
    let tuned = ch.tune_ship(ship);

    let turn_rate = tuned.turn_rate();
    let turn_90 = 90.0 / turn_rate;

    const TURN_TIME_SIGMA: f64 = 0.08;
    let turn_rate_sigma = TURN_TIME_SIGMA * turn_rate * turn_rate / 90.0;
    let beam_dps: f64 = tuned.weapons_of(&WeaponType::Beam).map(|w| w.dps).sum();
    let missile_dps: f64 = tuned.weapons_of(&WeaponType::Missile).map(|w| w.dps).sum();

    // Thousands-separated integer
    fn add_commas(n: i64) -> String {
        let neg = n < 0;
        let digits = n.unsigned_abs().to_string();
        let len = digits.len();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (len - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        if neg { format!("-{out}") } else { out }
    }

    fn yn(b: bool) -> &'static str {
        if b { "Yes" } else { "No" }
    }
    fn stat(label: &str, value_html: &str) -> String {
        format!("<div class=\"stat\"><span class=\"k\">{label}</span><span class=\"v\">{value_html}</span></div>")
    }
    fn group(title: &str, cells: &str) -> String {
        format!("<div class=\"stat-group\"><h4>{title}</h4><div class=\"stat-grid\">{cells}</div></div>")
    }
    // higher-is-better numeric token
    fn num(key: &str, val: f64, shown: &str) -> String {
        format!("<span data-stat=\"{key}\" data-val=\"{val}\">{shown}</span>")
    }
    // lower-is-better numeric token
    fn lo(key: &str, val: f64, shown: &str) -> String {
        format!("<span data-stat=\"{key}\" data-val=\"{val}\" data-invert=\"1\">{shown}</span>")
    }

    let warp_cell = |ks: &str, kc: &str, speed: f64, cost: Option<f64>| match cost {
        Some(c) => format!("{} (cost {})", num(ks, speed, &format!("{speed:.2}")), lo(kc, c, &format!("{c:.1}"))),
        None => num(ks, speed, &format!("{speed:.2}")),
    };
    let imp_cell = |ks: &str, kc: &str, speed: f64, c: f64| {
        format!("{} (cost {})", num(ks, speed, &format!("{speed:.3}")), lo(kc, c, &format!("{c:.2}")))
    };
    let shield_row = |kc: &str, kd: &str, cost: f64, def: f64| {
        format!("{} (Defense {})", lo(kc, cost, &format!("{cost:.0}")), num(kd, def, &format!("{def:.1}")))
    };

    let mut stats = String::new();

    stats.push_str(&group(
        "Identity",
        &(stat("Class", &tuned.class)
            + &stat("Sensor Class", &tuned.sensor_class)
            + &stat("Type", &tuned.ship_type)
            + &stat("Crew", &num("crew", tuned.crew, &format!("{:.0}", tuned.crew)))
            + &stat("Quota", &num("quota", tuned.quota as f64, &tuned.quota.to_string()))
            + &stat("Cost", &add_commas(tuned.cost))),
    ));

    stats.push_str(&group(
        "Hull",
        &(stat("Superstructure", &num("structure", tuned.structure, &format!("{:.0}", tuned.structure)))
            + &stat("Repair", &num("repair", tuned.repair, &format!("{:.0}", tuned.repair)))
            + &stat("Mass", &add_commas(tuned.mass))
            + &stat("Bay", &num("bay", tuned.bay as f64, &add_commas(tuned.bay)))
            + &stat("Cargo", &num("cargo", tuned.cargo as f64, &tuned.cargo.to_string()))),
    ));

    stats.push_str(&group(
        "Docking",
        &(stat("Has Land", yn(tuned.has_land))
            + &stat("Has Dock", yn(tuned.has_dock))
            + &stat("Can Land", yn(tuned.can_land))
            + &stat("Can Dock", yn(tuned.can_dock))),
    ));

    stats.push_str(&group(
        "Systems",
        &(stat("Firing", &num("firing", tuned.firing, &format!("{:.1}", tuned.firing)))
            + &stat("Fuel Efficiency", &num("fuel_eff", tuned.fuel_eff, &format!("{:.1}", tuned.fuel_eff)))
            + &stat("Stealth", &num("stealth", tuned.stealth, &format!("{:.1}", tuned.stealth)))
            + &stat("Cloak", &num("cloak_eff", tuned.cloak_eff, &format!("{:.1}", tuned.cloak_eff)))
            + &stat("Sensors", &num("sensors", tuned.sensors, &format!("{:.1}", tuned.sensors)))
            + &stat("Aux Max", &num("aux_max", tuned.aux_max, &format!("{:.1}", tuned.aux_max)))
            + &stat("Main Max", &num("main_max", tuned.main_max, &format!("{:.1}", tuned.main_max)))
            + &stat("Armor", &num("armor", tuned.armor, &format!("{:.1}", tuned.armor)))
            + &stat("Fuel Max", &num("fuel_max", tuned.fuel_max, &format!("{:.0}", tuned.fuel_max)))),
    ));

    let has_cloak = num("has_cloak", if tuned.has_cloak { 1.0 } else { 0.0 }, yn(tuned.has_cloak));
    let cloak_val = match tuned.cloak {
        Some(cost) => format!("{has_cloak} (cost: {})", lo("cloak_cost", cost, &format!("{cost:.1}"))),
        None => has_cloak,
    };
    stats.push_str(&group(
        "Capabilities",
        &(stat("LRS", yn(tuned.lrs))
            + &stat("SRS", yn(tuned.srs))
            + &stat("EW", yn(tuned.ew))
            + &stat("Trans", yn(tuned.trans))
            + &stat("Tractor", yn(tuned.tractor))
            + &stat("Cloak", &cloak_val)),
    ));

    let main_scaled = tuned.main * tuned.main_max / 100.0;
    let aux_scaled = tuned.aux * tuned.aux_max / 100.0;
    stats.push_str(&group(
        "Power &amp; Movement",
        &(stat("Main", &format!("{} ({main_scaled:.1})", num("main", tuned.main, &format!("{:.0}", tuned.main))))
            + &stat("Aux", &format!("{} ({aux_scaled:.1})", num("aux", tuned.aux, &format!("{:.0}", tuned.aux))))
            + &stat("Batt", &num("batt", tuned.batt, &format!("{:.1}", tuned.batt)))
            + &stat("MoveRatio", &lo("move_ratio", tuned.move_ratio, &format!("{:.2}", tuned.move_ratio)))
            + &stat("Turn rate", &num("turn_rate", turn_rate, &format!("{turn_rate:.2} &plusmn; {turn_rate_sigma:.2} deg/s")))
            + &stat("90&deg; turn", &lo("turn_90", turn_90, &format!("{turn_90:.2} &plusmn; {TURN_TIME_SIGMA:.2} s")))),
    ));

    let warp = match (tuned.warp_cruise, tuned.warp_emer, tuned.warp_max) {
        (Some(cruise), Some(emer), Some(max)) => {
            let mut c = stat("Cruise", &warp_cell("warp_cruise", "warp_cruise_cost", cruise, tuned.warp_cruise_cost))
                + &stat("Emergency", &warp_cell("warp_emer", "warp_emer_cost", emer, tuned.warp_emer_cost))
                + &stat("Max", &warp_cell("warp_max", "warp_max_cost", max, tuned.warp_max_cost));
            if let Some(t) = &tuned.warp_type {
                c += &stat("Type", t);
            }
            if !is_transwarp(ship) {
                if let Some((pc, pe, pm)) = ch.transwarp_projection(ship) {
                    let f = |key: &str, v: Option<f64>| match v {
                        Some(x) => num(key, x, &format!("{x:.2}")),
                        None => "—".to_string(),
                    };

                    c += &stat(
                        "w/ TWD",
                        &format!(
                            "{} / {} / {}",
                            f("twd_cruise", pc),
                            f("twd_emer", pe),
                            num("twd_max", pm, &format!("{pm:.2}"))
                        ),
                    );
                    let fc = |key: &str, v: Option<f64>| match v {
                        Some(x) => lo(key, x, &format!("{x:.1}")),
                        None => "—".to_string(),
                    };

                    // TWD cost = 0.1 * MR * TWDSpeed^2 
                    let twd_cost = |cost: Option<f64>, base: Option<f64>, twd: Option<f64>| -> Option<f64> {
                        let (_cost, base, twd) = (cost?, base?, twd?);
                        (base > 0.0).then(|| 0.1 * tuned.move_ratio * twd.powi(2))
                    };

                    c += &stat(
                        "TWD cost",
                        &format!(
                            "{} / {} / {}",
                            fc("twd_cruise_cost", twd_cost(tuned.warp_cruise_cost, ship.warp_cruise, pc)),
                            fc("twd_emer_cost", twd_cost(tuned.warp_emer_cost, ship.warp_emer, pe)),
                            fc("twd_max_cost", twd_cost(tuned.warp_max_cost, ship.warp_max, Some(pm)))
                        ),
                    );
                }
            }
            c
        }
        _ => stat("Warp", "none"),
    };
    stats.push_str(&group("Warp", &warp));

    stats.push_str(&group(
        "Impulse",
        &(stat("Cruise", &imp_cell("imp_cruise", "imp_cruise_cost", tuned.imp_cruise, tuned.imp_cruise_cost))
            + &stat("Emergency", &imp_cell("imp_emer", "imp_emer_cost", tuned.imp_emer, tuned.imp_emer_cost))
            + &stat("Max", &imp_cell("imp_max", "imp_max_cost", tuned.imp_max, tuned.imp_max_cost))),
    ));

    stats.push_str(&group(
        "Shields",
        &(stat("Max", &num("shield_max", tuned.shield_max, &format!("{:.0}", tuned.shield_max)))
            + &stat("Ratio", &num("shield_ratio", tuned.shield_ratio, &format!("{:.2}", tuned.shield_ratio)))
            + &stat("1x Cost", &shield_row("shield_cost_1x", "shield_def_1x", tuned.shield_cost_1x, tuned.shield_def_1x))
            + &stat("2x Cost", &shield_row("shield_cost_2x", "shield_def_2x", tuned.shield_cost_2x, tuned.shield_def_2x))
            + &stat("3x Cost", &shield_row("shield_cost_3x", "shield_def_3x", tuned.shield_cost_3x, tuned.shield_def_3x))
            + &stat("4x Cost", &shield_row("shield_cost_4x", "shield_def_4x", tuned.shield_cost_4x, tuned.shield_def_4x))),
    ));

    let weapons_summary = group(
        "Weapons",
        &(stat("Nominal Beam DPS", &num("nominal_beam_dps", beam_dps, &format!("{beam_dps:.1}")))
            + &stat("Nominal Missile DPS", &num("nominal_missile_dps", missile_dps, &format!("{missile_dps:.1}")))
            + &stat(
                "Nominal Total DPS",
                &num("nominal_total_dps", beam_dps + missile_dps, &format!("{:.1}", beam_dps + missile_dps)),
            )),
    );

    let weapon_table = |kind: &WeaponType| -> String {
        let mut t = String::new();
        t.push_str("<table class=\"weapons\"><tr><th>Arc</th><th>Cost</th><th>Range</th><th>Damage</th><th>Recycle</th><th>DPS</th></tr>");
        for w in tuned.weapons_of(kind) {
            t.push_str(&format!(
                "<tr><td>{}</td><td>{:.0}</td><td>{:.0}</td><td>{:.0}</td><td>{:.1}s</td><td>{:.1}</td></tr>",
                w.arc, w.cost, w.range, w.damage, w.recycle_time, w.dps
            ));
        }
        t.push_str("</table>");
        t
    };
    let beams = weapon_table(&WeaponType::Beam);
    let missiles = weapon_table(&WeaponType::Missile);

    Ok(format!(
        "<div class=\"ship-general\">{stats}</div>\
         <div class=\"weapons-summary\">{weapons_summary}</div>\
         <div class=\"weapons-cols\">\
           <div class=\"ship-table-col\"><h3>Beams</h3>{beams}</div>\
           <div class=\"ship-table-col\"><h3>Missiles</h3>{missiles}</div>\
         </div>"
    ))
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render_instantaneous(
    name: &str,
    eng: f64,
    tac: f64,
    helm: f64,
    oper: f64,
    sci: f64,
    dam: f64,
    wis: f64,
    timing: &str,
    bg: &str,
    width: u32,
    height: u32,
) -> Result<String, JsValue> {
    let ship = find_ship_or_err(name)?;
    let sig = run(ship, Character::new(eng, tac, helm, oper, sci, dam, wis), parse_timing(timing));
    let theme = theme_with_bg(bg);
    to_svg(width, height, |root| {
        plot_instantaneous(root, &sig, "Damage Timeline", &theme).map_err(jerr)
    })
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render_cumulative(
    names: Vec<String>,
    eng: f64,
    tac: f64,
    helm: f64,
    oper: f64,
    sci: f64,
    dam: f64,
    wis: f64,
    timing: &str,
    bg: &str,
    width: u32,
    height: u32,
) -> Result<String, JsValue> {
    let ch = Character::new(eng, tac, helm, oper, sci, dam, wis);
    let mut sims: Vec<(String, Rc<DamageSignal>)> = Vec::with_capacity(names.len());
    for name in &names {
        let ship = find_ship_or_err(name)?;
        sims.push((ship.name.clone(), run(ship, ch.clone(), parse_timing(timing))));
    }
    let theme = theme_with_bg(bg);
    to_svg(width, height, |root| {
        plot_cumulative(root, &sims, "Cumulative Damage Outputs", &theme).map_err(jerr)
    })
}


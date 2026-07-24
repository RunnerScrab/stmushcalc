//! Usage:  shipdb <name>[,<name>...] [path/to/ships.db]
//!             [--eng N --tac N --helm N --oper N --sci N --dam N --wis N]
//!
//! To build the ship database from a @listspecs log instead of plotting:
//!     shipdb --parse listspecs.log [--sqlite | --bincode]
//!
//! Writes the default catalog file (ships.db, or ships.bin with --bincode),
//! creating it if absent and upserting ships by dbref if it already exists.
//!
//! Each name matches the first ship (in dbref order) whose name contains it,
//! case-insensitive. Passing a list of ships separated by comma will produce
//! a single plot with the cumulative damage of each ship in the list. Passing
//! one ship will output its damage pulse train in time domain
//!
//! You can pass con skills to simulate ship tuning; they default to 0 bonus, i.e.
//! giving untuned base ship specs for those in that console's domain, 10 wisdom

use anyhow::{bail, Context, Result};
use compact_str::CompactString;
use fxhash::FxHashMap;
use plotters::prelude::*;
use rayon::prelude::*;
use shipdb::plot::{render_cumulative, render_instantaneous, LIGHT};
use shipdb::{
    load_ships, open, simulate_damage, Character, DamageSignal, SimConfig, TurnRng,
    DEFAULT_HORIZON_SECS as HORIZON_SECS, DEFAULT_RNG_SEED as RNG_SEED,
    DEFAULT_SAMPLE_DT as SAMPLE_DT, DEFAULT_VOLLEY_WINDOW as VOLLEY_WINDOW,
};
use smallvec::SmallVec;
use std::{env, fmt::Write};

const DEFAULT_DBNAME: &str = "ships.db";

#[derive(Clone, Copy)]
enum ImgFmt {
    Svg,
    Png,
}

impl ImgFmt {
    fn ext(self) -> &'static str {
        match self {
            ImgFmt::Svg => "svg",
            ImgFmt::Png => "png",
        }
    }
}

fn make_char_flagstr(character: &Option<Character>) -> anyhow::Result<String> {
    let mut flagstr: String = String::with_capacity(128);

    if let Some(ch) = character {
        write!(&mut flagstr, ", Wis: {}, Helm: {}, Tac: {}, Eng: {}, Opr: {}, Dam: {}, Sci: {}",
            ch.wisdom, ch.helm, ch.tactical, ch.engineering, ch.operations, ch.damage_control,
            ch.science)?;
        Ok(flagstr)
    } else {
        Ok("".to_string())
    }
}

fn plot_inst_damage_signal(sig: &DamageSignal, ship: &shipdb::Ship, fmt: ImgFmt) -> anyhow::Result<()> {
    let flagstr = make_char_flagstr(&ship.character)?;
    let filename = format!("{}{}.{}", ship.name, flagstr, fmt.ext());
    let caption = format!("{} instantaneous damage output{}", ship.name, flagstr);
    match fmt {
        ImgFmt::Svg => {
            let root = SVGBackend::new(&filename, (1000, 500)).into_drawing_area();
            render_instantaneous(&root, sig, &caption, &LIGHT)?;
            root.present()?;
        }
        ImgFmt::Png => {
            let root = BitMapBackend::new(&filename, (1000, 500)).into_drawing_area();
            render_instantaneous(&root, sig, &caption, &LIGHT)?;
            root.present()?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum OutFmt {
    Sqlite,
    Bincode,
}

const DEFAULT_BINNAME: &str = "ships.bin";

/// Parse a @listspecs log into the default database file, creating it if absent
/// and upserting by dbref if it already exists
fn run_parse(log_path: &str, fmt: OutFmt) -> Result<()> {
    let text = std::fs::read_to_string(log_path)
        .with_context(|| format!("reading log {log_path}"))?;
    let parsed = shipdb::logparse::parse_logs(text.as_bytes());
    if parsed.is_empty() {
        bail!("no ships parsed from {log_path}");
    }

    match fmt {
        OutFmt::Sqlite => {
            shipdb::save_ships(&parsed, DEFAULT_DBNAME)?;
            println!("parsed {} ships into {DEFAULT_DBNAME}", parsed.len());
        }
        OutFmt::Bincode => {
            let mut by_dbref: std::collections::BTreeMap<i64, shipdb::Ship> =
                std::collections::BTreeMap::new();
            if std::path::Path::new(DEFAULT_BINNAME).exists() {
                for s in shipdb::load_snapshot(DEFAULT_BINNAME)? {
                    by_dbref.insert(s.dbref, s);
                }
            }
            for s in parsed {
                by_dbref.insert(s.dbref, s);
            }
            let merged: Vec<shipdb::Ship> = by_dbref.into_values().collect();
            shipdb::save_snapshot(&merged, DEFAULT_BINNAME)?;
            println!("parsed into {DEFAULT_BINNAME} ({} ships total)", merged.len());
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut arguments: SmallVec<[CompactString; 8]> = SmallVec::new();
    let mut flags: FxHashMap<String, f64> = FxHashMap::default();
    let mut fmt = ImgFmt::Svg;
    let mut parse_log: Option<String> = None;
    let mut out_fmt = OutFmt::Sqlite;
    let mut it = env::args().skip(1);

    while let Some(a) = it.next() {
        if let Some(key) = a.strip_prefix("--") {
            match key {
                "png" => {
                    fmt = ImgFmt::Png;
                    continue;
                }
                "svg" => {
                    fmt = ImgFmt::Svg;
                    continue;
                }
                "sqlite" => {
                    out_fmt = OutFmt::Sqlite;
                    continue;
                }
                "bincode" => {
                    out_fmt = OutFmt::Bincode;
                    continue;
                }
                "parse" => {
                    let Some(log) = it.next() else {
                        bail!("--parse needs a log file path");
                    };
                    parse_log = Some(log);
                    continue;
                }
                _ => {}
            }
            let Some(val) = it.next().and_then(|v| v.parse::<f64>().ok()) else {
                bail!("flag --{key} needs a numeric value");
            };
            flags.insert(key.to_string(), val);
        } else {
            arguments.push(a.into());
        }
    }

    if let Some(log) = parse_log {
        return run_parse(&log, out_fmt);
    }

    let Some(query) = arguments.first() else {
        bail!("usage: shipdb <ship-name> [db-path] [--eng N --tac N --helm N --oper N --sci N --dam N --wis N]");
    };
    let path = arguments.get(1).map(CompactString::as_str).unwrap_or(DEFAULT_DBNAME);

    let conn = open(path)?;
    let ships = load_ships(&conn)?;

    // Simulate con skills if any were supplied; unspecified consoles
    // default to 0 points and wisdom defaults to 10 for 0 bonus 
    let get = |k: &str, d: f64| *flags.get(k).unwrap_or(&d);
    let character = ["eng", "tac", "helm", "oper", "sci", "dam", "wis"]
        .iter()
        .any(|k| flags.contains_key(*k))
        .then(|| {
            Character::new(
                get("eng", 0.0),
                get("tac", 0.0),
                get("helm", 0.0),
                get("oper", 0.0),
                get("sci", 0.0),
                get("dam", 0.0),
                get("wis", 10.0),
            )
        });

    if let Some(c) = &character {
        println!(
            "Crew tuning (wis-mod {}): ENG {:.1}%  TACT {:.1}%  HELM {:.1}%  OPER {:.1}%  SCI {:.1}%  DAM {:.1}%",
            c.wisdom_mod(),
            c.engineering_bonus() * 100.0,
            c.tactical_bonus() * 100.0,
            c.helm_bonus() * 100.0,
            c.operations_bonus() * 100.0,
            c.science_bonus() * 100.0,
            c.damage_control_bonus() * 100.0,
        );
    }

    // Tokenize the comma-separated ship list, tuning each
    // with the same console skill/wis arguments
    let mut fleet: Vec<shipdb::Ship> = Vec::with_capacity(96);
    for name in query.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let needle = name.to_lowercase();
        let Some(found) = ships
            .iter()
            .find(|s| s.name.to_lowercase().contains(&needle))
        else {
            bail!("no ship matching {:?} in {}", name, path);
        };
        fleet.push(match &character {
            Some(c) => found.clone().with_character(c.clone()).effective(),
            None => found.clone(),
        });
    }
    if fleet.is_empty() {
        bail!("no ship names given");
    }

    if fleet.len() == 1 {
        // Single ship, plots damage signal
        report_single(&fleet[0], character.is_some(), fmt)?;
    } else {
        // Simulate multiple ships, and use rayon to parallelize 
        let sims: SmallVec<[(String, DamageSignal); 16]> = fleet
            .par_iter()
            .map(|s| {
                let mut rng = TurnRng::new(RNG_SEED);
                let cfg = SimConfig::new(
                    HORIZON_SECS,
                    SAMPLE_DT,
                    shipdb::TurnTimingModel::Anticipatory,
                    VOLLEY_WINDOW,
                );
                (s.name.clone(), simulate_damage(s, &mut rng, &cfg))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .collect();
        for (name, sig) in &sims {
            let total = sig.total;
            println!(
                "{}: {:.0} total damage, {:.1} sustained DPS",
                name,
                total,
                total / HORIZON_SECS
            );
        }
        plot_cumulative_damage_outputs(&sims, &character, fmt)?;
    }

    Ok(())
}

fn report_single(ship: &shipdb::Ship, tuned: bool, fmt: ImgFmt) -> Result<()> {
    println!(
        "{} (#{}) - {} {}, {} weapons, {} DPS {:.1}",
        ship.name,
        ship.dbref,
        ship.class,
        ship.ship_type,
        ship.weapons.len(),
        if tuned { "tuned" } else { "catalog" },
        ship.total_dps(),
    );
    for w in &ship.weapons {
        println!(
            "  {:?} slot {}: {} dmg @ {:.0} range, arc {}, recycle {:.1}s ({:.1} dps)",
            w.weapon_type, w.slot, w.damage, w.range, w.arc, w.recycle_time, w.dps
        );
    }
    for (label, timing) in [
        ("Anticipatory", shipdb::TurnTimingModel::Anticipatory),
        ("Reactive", shipdb::TurnTimingModel::Reactive),
    ] {
        println!("{label}:");
        let mut rng = TurnRng::new(RNG_SEED);
        let cfg = SimConfig::new(HORIZON_SECS, SAMPLE_DT, timing, VOLLEY_WINDOW);
        let sig = simulate_damage(ship, &mut rng, &cfg);
        let rotation: String = sig.rotation.iter().map(|f| f.label()).collect();
        let total = sig.total;
        let peak = sig.peak;

        // We no longer actually sample a damage signal, but use a sparse, line-segment
        // representation of instantaneous damage
        println!(
            "Simulated {:.0}s @ {:.1}s samples ({} shots), arc rotation [{}]:",
            HORIZON_SECS,
            SAMPLE_DT,
            sig.events.len(),
            rotation,
        );
        println!(
            "  {} shots, {:.0} total damage, {:.1} sustained DPS, {:.1}s turning, {:.0} peak volley",
            sig.events.len(),
            total,
            total / HORIZON_SECS,
            sig.turn_time_total,
            peak,
        );
        if matches!(timing, shipdb::TurnTimingModel::Anticipatory) {
            plot_inst_damage_signal(&sig, ship, fmt)?;
        }
    }
    Ok(())
}

fn plot_cumulative_damage_outputs(
    sims: &[(String, DamageSignal)],
    character: &Option<Character>,
    fmt: ImgFmt,
) -> anyhow::Result<()> {
    let flagstr = make_char_flagstr(character)?;
    let filename = format!("cumulative{}.{}", flagstr, fmt.ext());
    let caption = format!("Cumulative Damage Out{}", flagstr);
    match fmt {
        ImgFmt::Svg => {
            let root = SVGBackend::new(&filename, (2000, 1000)).into_drawing_area();
            render_cumulative(&root, sims, &caption, &LIGHT)?;
            root.present()?;
        }
        ImgFmt::Png => {
            let root = BitMapBackend::new(&filename, (2000, 1000)).into_drawing_area();
            render_cumulative(&root, sims, &caption, &LIGHT)?;
            root.present()?;
        }
    }
    Ok(())
}


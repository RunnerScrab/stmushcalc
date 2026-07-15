use anyhow::{Context, Result};
use fxhash::FxHashMap;
use rusqlite::{params, Connection, OpenFlags, Row};
use std::path::Path;

use crate::ship::Ship;
use crate::weapon::{Weapon, WeaponType};

/// Open db read-only
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let path = path.as_ref();
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening database {}", path.display()))
}

/// Load every ship and weapon 
pub fn load_ships(conn: &Connection) -> Result<Vec<Ship>> {

    // dbref as foreign key
    let mut by_ship: FxHashMap<i64, Vec<Weapon>> = FxHashMap::default();
    let mut wstmt = conn.prepare(
        "SELECT dbref, weapon_type, slot, cost, range, arc, damage, time, dps \
         FROM weapons ORDER BY dbref, weapon_type, slot",
    )?;
    let wrows = wstmt.query_map([], weapon_from_row)?;
    for row in wrows {
        let (dbref, w) = row.context("reading a weapons row")?;
        by_ship.entry(dbref).or_default().push(w);
    }

    // attach weapons to ships 
    let mut sstmt = conn.prepare("SELECT * FROM ships ORDER BY dbref")?;
    let rows = sstmt.query_map([], ship_from_row)?;
    let mut ships = Vec::new();
    for row in rows {
        let mut ship = row.context("reading a ships row")?;
        if let Some(w) = by_ship.remove(&ship.dbref) {
            ship.weapons = w;
        }
        ships.push(ship);
    }
    Ok(ships)
}

/// Map one weapons row to (dbref, Weapon)
fn weapon_from_row(row: &Row) -> rusqlite::Result<(i64, Weapon)> {
    let dbref: i64 = row.get("dbref")?;
    let w = Weapon {
        weapon_type: WeaponType::from_db(&row.get::<_, String>("weapon_type")?),
        slot: row.get("slot")?,
        cost: row.get("cost")?,
        range: row.get("range")?,
        arc: row.get("arc")?,
        damage: row.get("damage")?,
        recycle_time: row.get("time")?,
        dps: row.get("dps")?,
    };
    Ok((dbref, w))
}

/// Map one ship row to a Ship struct instance 
fn ship_from_row(row: &Row) -> rusqlite::Result<Ship> {
    Ok(Ship {
        dbref: row.get("dbref")?,
        name: row.get("name")?,
        category: row.get("category")?,
        art: row.get("art")?,
        class: row.get("class")?,
        sensor_class: row.get("sensor_class")?,
        ship_type: row.get("type")?,
        crew: row.get("crew")?,
        crew_tuned: row.get("crew_tuned")?,
        quota: row.get("quota")?,
        cost: row.get("cost")?,

        structure: row.get("structure")?,
        repair: row.get("repair")?,
        mass: row.get("mass")?,
        bay: row.get("bay")?,
        cargo: row.get("cargo")?,

        has_land: row.get("has_land")?,
        has_dock: row.get("has_dock")?,
        can_land: row.get("can_land")?,
        can_dock: row.get("can_dock")?,

        firing: row.get("firing")?,
        fuel_eff: row.get("fuel_eff")?,
        stealth: row.get("stealth")?,
        cloak_eff: row.get("cloak_eff")?,
        sensors: row.get("sensors")?,
        aux_max: row.get("aux_max")?,
        main_max: row.get("main_max")?,
        armor: row.get("armor")?,
        fuel_max: row.get("fuel_max")?,

        lrs: row.get("lrs")?,
        srs: row.get("srs")?,
        ew: row.get("ew")?,
        trans: row.get("trans")?,
        tractor: row.get("tractor")?,
        has_cloak: row.get("has_cloak")?,
        cloak: row.get("cloak")?,

        main: row.get("main")?,
        aux: row.get("aux")?,
        batt: row.get("batt")?,
        move_ratio: row.get("move_ratio")?,

        warp_cruise: row.get("warp_cruise")?,
        warp_emer: row.get("warp_emer")?,
        warp_max: row.get("warp_max")?,
        warp_cruise_cost: row.get("warp_cruise_cost")?,
        warp_emer_cost: row.get("warp_emer_cost")?,
        warp_max_cost: row.get("warp_max_cost")?,
        warp_type: row.get("warp_type")?,

        imp_cruise: row.get("imp_cruise")?,
        imp_emer: row.get("imp_emer")?,
        imp_max: row.get("imp_max")?,
        imp_cruise_cost: row.get("imp_cruise_cost")?,
        imp_emer_cost: row.get("imp_emer_cost")?,
        imp_max_cost: row.get("imp_max_cost")?,

        shield_max: row.get("shield_max")?,
        shield_ratio: row.get("shield_ratio")?,
        shield_def_1x: row.get("shield_def_1x")?,
        shield_def_2x: row.get("shield_def_2x")?,
        shield_def_3x: row.get("shield_def_3x")?,
        shield_def_4x: row.get("shield_def_4x")?,
        shield_cost_1x: row.get("shield_cost_1x")?,
        shield_cost_2x: row.get("shield_cost_2x")?,
        shield_cost_3x: row.get("shield_cost_3x")?,
        shield_cost_4x: row.get("shield_cost_4x")?,

        beams_count: row.get("beams_count")?,
        beams_dps: row.get("beams_dps")?,
        missiles_count: row.get("missiles_count")?,
        missiles_dps: row.get("missiles_dps")?,

        source_log: row.get("source_log")?,
        source_line: row.get("source_line")?,
        parsed_at: row.get("parsed_at")?,

        weapons: Vec::new(),
        character: None,
    })
}

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS ships (
    dbref INTEGER PRIMARY KEY,
    name TEXT, category TEXT, art TEXT, class TEXT, sensor_class TEXT, type TEXT,
    crew REAL, crew_tuned INTEGER, quota INTEGER, cost INTEGER,
    structure REAL, repair REAL, mass INTEGER, bay INTEGER, cargo INTEGER,
    has_land INTEGER, has_dock INTEGER, can_land INTEGER, can_dock INTEGER,
    firing REAL, fuel_eff REAL, stealth REAL, cloak_eff REAL, sensors REAL,
    aux_max REAL, main_max REAL, armor REAL, fuel_max REAL,
    lrs INTEGER, srs INTEGER, ew INTEGER, trans INTEGER, tractor INTEGER,
    has_cloak INTEGER, cloak REAL,
    main REAL, aux REAL, batt REAL, move_ratio REAL,
    warp_cruise REAL, warp_emer REAL, warp_max REAL,
    warp_cruise_cost REAL, warp_emer_cost REAL, warp_max_cost REAL, warp_type TEXT,
    imp_cruise REAL, imp_emer REAL, imp_max REAL,
    imp_cruise_cost REAL, imp_emer_cost REAL, imp_max_cost REAL,
    shield_max REAL, shield_ratio REAL,
    shield_def_1x REAL, shield_def_2x REAL, shield_def_3x REAL, shield_def_4x REAL,
    shield_cost_1x REAL, shield_cost_2x REAL, shield_cost_3x REAL, shield_cost_4x REAL,
    beams_count INTEGER, beams_dps REAL, missiles_count INTEGER, missiles_dps REAL,
    source_log TEXT, source_line INTEGER, parsed_at TEXT
);
CREATE TABLE IF NOT EXISTS weapons (
    dbref INTEGER, weapon_type TEXT, slot INTEGER,
    cost REAL, range REAL, arc TEXT, damage REAL, time REAL, dps REAL,
    PRIMARY KEY (dbref, weapon_type, slot),
    FOREIGN KEY (dbref) REFERENCES ships(dbref) ON DELETE CASCADE
);";

const SHIP_COLUMNS: &str = "dbref, name, category, art, class, sensor_class, type, crew, \
    crew_tuned, quota, cost, structure, repair, mass, bay, cargo, has_land, has_dock, can_land, \
    can_dock, firing, fuel_eff, stealth, cloak_eff, sensors, aux_max, main_max, armor, fuel_max, \
    lrs, srs, ew, trans, tractor, has_cloak, cloak, main, aux, batt, move_ratio, warp_cruise, \
    warp_emer, warp_max, warp_cruise_cost, warp_emer_cost, warp_max_cost, warp_type, imp_cruise, \
    imp_emer, imp_max, imp_cruise_cost, imp_emer_cost, imp_max_cost, shield_max, shield_ratio, \
    shield_def_1x, shield_def_2x, shield_def_3x, shield_def_4x, shield_cost_1x, shield_cost_2x, \
    shield_cost_3x, shield_cost_4x, beams_count, beams_dps, missiles_count, missiles_dps, \
    source_log, source_line, parsed_at";

const SHIP_COL_COUNT: usize = 70;

/// Create the schema if absent and upsert every ship (and its weapons) by dbref
pub fn save_ships(ships: &[Ship], path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let mut conn = Connection::open(path)
        .with_context(|| format!("opening {} for writing", path.display()))?;
    conn.execute_batch(SCHEMA)?;

    let placeholders = vec!["?"; SHIP_COL_COUNT].join(", ");
    let insert_ship = format!("INSERT OR REPLACE INTO ships ({SHIP_COLUMNS}) VALUES ({placeholders})");

    let tx = conn.transaction()?;
    {
        let mut ship_stmt = tx.prepare(&insert_ship)?;
        let mut del_weapons = tx.prepare("DELETE FROM weapons WHERE dbref = ?1")?;
        let mut ins_weapon = tx.prepare(
            "INSERT OR REPLACE INTO weapons \
             (dbref, weapon_type, slot, cost, range, arc, damage, time, dps) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for s in ships {
            ship_stmt.execute(params![
                s.dbref, s.name, s.category, s.art, s.class, s.sensor_class, s.ship_type,
                s.crew, s.crew_tuned, s.quota, s.cost,
                s.structure, s.repair, s.mass, s.bay, s.cargo,
                s.has_land, s.has_dock, s.can_land, s.can_dock,
                s.firing, s.fuel_eff, s.stealth, s.cloak_eff, s.sensors, s.aux_max, s.main_max,
                s.armor, s.fuel_max,
                s.lrs, s.srs, s.ew, s.trans, s.tractor, s.has_cloak, s.cloak,
                s.main, s.aux, s.batt, s.move_ratio,
                s.warp_cruise, s.warp_emer, s.warp_max,
                s.warp_cruise_cost, s.warp_emer_cost, s.warp_max_cost, s.warp_type,
                s.imp_cruise, s.imp_emer, s.imp_max,
                s.imp_cruise_cost, s.imp_emer_cost, s.imp_max_cost,
                s.shield_max, s.shield_ratio,
                s.shield_def_1x, s.shield_def_2x, s.shield_def_3x, s.shield_def_4x,
                s.shield_cost_1x, s.shield_cost_2x, s.shield_cost_3x, s.shield_cost_4x,
                s.beams_count, s.beams_dps, s.missiles_count, s.missiles_dps,
                s.source_log, s.source_line, s.parsed_at,
            ])?;
            del_weapons.execute(params![s.dbref])?;
            for w in &s.weapons {
                ins_weapon.execute(params![
                    s.dbref, w.weapon_type.to_db(), w.slot, w.cost, w.range, w.arc,
                    w.damage, w.recycle_time, w.dps
                ])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}


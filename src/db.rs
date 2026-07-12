use anyhow::{Context, Result};
use fxhash::FxHashMap;
use rusqlite::{Connection, OpenFlags, Row};
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


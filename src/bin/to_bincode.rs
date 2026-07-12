//! Converts the ship SQLite database into a bincode format, for use
//! with the WASM bindings

use anyhow::{bail, Result};
use std::env;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(db_path) = args.next() else {
        bail!("usage: to_bincode <ships.db> [out.bin]");
    };
    let out_path = args.next().unwrap_or_else(|| "ships.bin".to_string());

    let conn = shipdb::open(&db_path)?;
    let ships = shipdb::load_ships(&conn)?;
    shipdb::save_snapshot(&ships, &out_path)?;

    println!("wrote {} ships to {}", ships.len(), out_path);
    Ok(())
}


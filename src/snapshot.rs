//! Flat bincode snapshot of the ship catalog, as an alternative to `db`'s sqlite path

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use anyhow::{Context, Result};

use crate::ship::Ship;

pub fn save_snapshot(ships: &[Ship], path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    bincode::serialize_into(BufWriter::new(file), ships)
        .with_context(|| format!("encoding snapshot to {}", path.display()))
}

pub fn load_snapshot(path: impl AsRef<Path>) -> Result<Vec<Ship>> {
    let path = path.as_ref();
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    bincode::deserialize_from(BufReader::new(file))
        .with_context(|| format!("decoding snapshot from {}", path.display()))
}

pub fn ships_from_bytes(bytes: &[u8]) -> Result<Vec<Ship>> {
    bincode::deserialize(bytes).context("decoding snapshot bytes")
}

pub fn ships_to_bytes(ships: &[Ship]) -> Result<Vec<u8>> {
    bincode::serialize(ships).context("encoding snapshot bytes")
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;
    use crate::db;

    // Round-trips the real ships.db through bincode and spot-checks fields
    // that would catch truncation, field misordering, or float corruption
    #[test]
    fn snapshot_matches_sqlite_source() {
        let conn = db::open("ships.db").unwrap();
        let original = db::load_ships(&conn).unwrap();
        assert!(!original.is_empty());

        let tmp = std::env::temp_dir().join("shipdb_snapshot_test.bin");
        save_snapshot(&original, &tmp).unwrap();
        let restored = load_snapshot(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(original.len(), restored.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert_eq!(a.dbref, b.dbref);
            assert_eq!(a.name, b.name);
            assert_eq!(a.structure, b.structure);
            assert_eq!(a.move_ratio, b.move_ratio);
            assert_eq!(a.weapons.len(), b.weapons.len());
            assert_eq!(a.total_dps(), b.total_dps());
        }
    }
}

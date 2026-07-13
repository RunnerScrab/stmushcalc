mod character;
mod combat;
#[cfg(feature = "cli")]
mod db;
#[cfg(feature = "parse")]
pub mod logparse;
#[cfg(feature = "plot")]
pub mod plot;
mod rng;
mod ship;
mod snapshot;
mod weapon;

pub use character::{is_transwarp, Character};
pub use combat::{simulate_damage, DamageSignal, Face, Fire, SimConfig, TurnTimingModel};
#[cfg(feature = "cli")]
pub use db::{load_ships, open};
pub use rng::TurnRng;
pub use ship::Ship;
pub use snapshot::{load_snapshot, save_snapshot, ships_from_bytes, ships_to_bytes};
pub use weapon::{Weapon, WeaponType};

/// Default simulation horizon, in seconds
pub const DEFAULT_HORIZON_SECS: f64 = 400.0;

/// Default sample spacing, in seconds
pub const DEFAULT_SAMPLE_DT: f64 = 0.1;

/// Default consolidation window for a single volley
pub const DEFAULT_VOLLEY_WINDOW: f64 = 0.4;

/// Seed used for the turn-time RNG so runs are reproducible
pub const DEFAULT_RNG_SEED: u64 = 0xFEEDFACEDEADBEEF;


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Weapon {
    pub weapon_type: WeaponType,
    pub slot: i64,
    pub cost: f64,
    pub range: f64,
    pub arc: String, // e.g. F, FAPSD, A
    pub damage: f64,
    pub recycle_time: f64,
    pub dps: f64,
}

impl Weapon {
    /// Whether this weapon is on the forward arc
    pub fn is_forward(&self) -> bool {
        self.arc.contains('F')
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WeaponType {
    Beam,
    Missile,
    Other(String), // AFAIK all weapons are either beams or missiles 
}

impl WeaponType {

    #[inline]
    pub fn from_db(s: &str) -> Self {
        match s {
            "beam" => WeaponType::Beam,
            "missile" => WeaponType::Missile,
            other => WeaponType::Other(other.to_string()),
        }
    }

    #[inline]
    pub fn to_db(&self) -> &str {
        match self {
            WeaponType::Beam => "beam",
            WeaponType::Missile => "missile",
            WeaponType::Other(s) => s,
        }
    }
}


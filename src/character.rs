//! Each console yields a tuning bonus. Most of these formulas come directly from PoF mar'Qon's
//! spreadsheet. Characters can be "attached" to a ship to modify its specs

use crate::ship::Ship;

/// A character's console skills and total wisdom
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Character {
    pub engineering: f64,
    pub tactical: f64,
    pub helm: f64,
    pub operations: f64,
    pub science: f64,
    pub damage_control: f64,
    /// Total wisdom score (base + equipment buffs), *not* bonus
    pub wisdom: f64,
}

impl Character {

    pub fn new(eng: f64, tac: f64, helm: f64, oper: f64, sci: f64, dam: f64, wis: f64) -> Self {
        Self {
            engineering: eng,
            tactical: tac,
            helm,
            operations: oper,
            science: sci,
            damage_control: dam,
            wisdom: wis,
        }
    }

    /// Ability modifier from the wisdom score: floor((wisdom - 10) / 2)
    #[inline(always)]
    pub fn wisdom_mod(&self) -> f64 {
        ((self.wisdom - 10.0) / 2.0).floor()
    }

    /// Tuning bonus for a console
    /// sqrt((50 + skill + wisdom_mod) / 50) - 1
    #[inline(always)]
    fn bonus(&self, skill: f64) -> f64 {
        ((50.0 + skill + self.wisdom_mod()) / 50.0).sqrt() - 1.0
    }

    #[inline(always)]
    pub fn engineering_bonus(&self) -> f64 {
        self.bonus(self.engineering)
    }

    #[inline(always)]
    pub fn tactical_bonus(&self) -> f64 {
        self.bonus(self.tactical)
    }

    #[inline(always)]
    pub fn helm_bonus(&self) -> f64 {
        self.bonus(self.helm)
    }

    #[inline(always)]
    pub fn operations_bonus(&self) -> f64 {
        self.bonus(self.operations)
    }

    #[inline(always)]
    pub fn science_bonus(&self) -> f64 {
        self.bonus(self.science)
    }

    #[inline(always)]
    pub fn damage_control_bonus(&self) -> f64 {
        self.bonus(self.damage_control)
    }

    /// Apply this character's tunings to ship, returning the tuned copy
    pub fn tune_ship(&self, ship: &Ship) -> Ship {
        let eng = self.engineering_bonus();
        let tact = self.tactical_bonus();
        let helm = self.helm_bonus();
        let oper = self.operations_bonus();
        let sci = self.science_bonus();
        let dam = self.damage_control_bonus();

        let mut tuned = ship.clone();

        tuned.main_max = ship.main_max * (1.0 + eng);
        tuned.aux_max = ship.aux_max * (1.0 + eng);
        tuned.batt = ship.batt * (1.0 + eng);
        tuned.fuel_eff = ship.fuel_eff * (1.0 + 3.0 * eng);

        let factor = warp_interp_factor(eng);
        let bump = if is_transwarp(ship) { 8.0 } else { 0.0 };
        if let Some(base_max) = ship.warp_max {
            let (cruise, emer, max) = tuned_warp(ship.warp_cruise, ship.warp_emer, base_max, factor, bump);
            tuned.warp_cruise = cruise;
            tuned.warp_emer = emer;
            tuned.warp_max = Some(max);
        }

        let r = eng / (1.0 + eng);
        tuned.imp_cruise = ship.imp_cruise + r * (1.0 - ship.imp_cruise);
        tuned.imp_emer = ship.imp_emer + r * (1.0 - ship.imp_emer);
        tuned.imp_max = ship.imp_max + r * (1.0 - ship.imp_max);

        for w in &mut tuned.weapons {
            if w.recycle_time > 0.0 {
                w.recycle_time /= 1.0 + tact;
                w.dps = w.damage / w.recycle_time;
            }
        }

        tuned.move_ratio = round_down(ship.move_ratio / (1.0 + helm), 7);
        tuned.stealth = ship.stealth * (1.0 + 1.5 * helm);

        tuned.shield_max = (ship.shield_max * (1.0 + oper * 0.333_333)).round();
        if ship.shield_max > 0.0 {
            tuned.shield_ratio = ship.shield_ratio * (tuned.shield_max / ship.shield_max);
        }
        tuned.armor = ship.armor * (1.0 + oper);
        tuned.cargo = (ship.cargo as f64 * (1.0 + oper)).round() as i64;

        tuned.firing = ship.firing * (1.0 + sci);
        tuned.sensors = ship.sensors * (1.0 + sci);
        tuned.cloak_eff = ship.cloak_eff * (1.0 + sci);

        tuned.structure = (ship.structure * (1.0 + dam)).round();
        tuned.repair = (ship.repair * (1.0 + dam)).round();

        tuned.character = None;
        tuned
    }

    /// Warp cruise/emergency/max this ship would reach with a transwarp drive
    pub fn transwarp_projection(&self, ship: &Ship) -> Option<(Option<f64>, Option<f64>, f64)> {
        let factor = warp_interp_factor(self.engineering_bonus());
        let base_max = ship.warp_max?;
        Some(tuned_warp(ship.warp_cruise, ship.warp_emer, base_max, factor, 8.0))
    }
}

/// Whether a ship's warp drive is a transwarp drive
#[inline(always)]
pub fn is_transwarp(ship: &Ship) -> bool {
    ship.warp_type
        .as_deref()
        .is_some_and(|w| w.to_lowercase().contains("transwarp"))
}

#[inline(always)]
fn warp_interp_factor(eng_bonus: f64) -> f64 {
    1.0 - 20.0 / (20.0 + eng_bonus * 100.0)
}

#[inline]
fn tuned_warp(
    cruise: Option<f64>,
    emer: Option<f64>,
    base_max: f64,
    factor: f64,
    bump: f64,
) -> (Option<f64>, Option<f64>, f64) {
    let max = base_max + bump + 4.0 * factor;
    (
        cruise.map(|c| c + factor * (max - c)),
        emer.map(|e| (e + bump) + factor * (max - (e + bump))),
        max,
    )
}

/// Truncate x toward zero at decimals places
#[inline(always)]
fn round_down(x: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (x * f).trunc() / f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Character {
        Character {
            engineering: 30.0,
            tactical: 29.0,
            helm: 30.0,
            operations: 20.0,
            science: 20.0,
            damage_control: 20.0,
            wisdom: 40.0,
        }
    }

    #[test]
    fn wisdom_modifier_matches_sheet() {
        assert_eq!(sample().wisdom_mod(), 15.0);
        let low = Character {
            wisdom: 8.0,
            ..sample()
        };
        assert_eq!(low.wisdom_mod(), -1.0);
    }

    #[test]
    fn engineering_bonus_matches_sheet() {
        // sqrt((50 + 30 + 15) / 50) - 1 = sqrt(1.9) - 1 ~= 0.37840
        assert!((sample().engineering_bonus() - 0.378_404).abs() < 1e-5);
    }

    #[test]
    fn zero_skill_and_no_wisdom_is_no_bonus() {
        let plain = Character {
            engineering: 0.0,
            tactical: 0.0,
            helm: 0.0,
            operations: 0.0,
            science: 0.0,
            damage_control: 0.0,
            wisdom: 10.0,
        };
        assert!(plain.engineering_bonus().abs() < 1e-5);
    }

    #[test]
    fn warp_and_transwarp_match_sheet() {
        // Check that we match mar'Qon's spreadsheet
        let factor = warp_interp_factor(sample().engineering_bonus());
        let (sc, se, sm) = tuned_warp(Some(14.1), Some(16.2), 18.3, factor, 0.0);
        assert!((sc.unwrap() - 18.5597528972732).abs() < 1e-9);
        assert!((se.unwrap() - 19.2858878556762).abs() < 1e-9);
        assert!((sm - 20.9168857935181).abs() < 1e-9);
        let (tc, te, tm) = tuned_warp(Some(14.1), Some(16.2), 18.3, factor, 8.0);
        assert!((tc.unwrap() - 23.7935244843093).abs() < 1e-9);
        assert!((te.unwrap() - 27.2858878556762).abs() < 1e-9);
        assert!((tm - 28.9168857935181).abs() < 1e-9);
    }
}


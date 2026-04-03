use crate::grid::Grid;
use crate::unit::Unit;

/// Result of a combat action.
#[derive(Debug, PartialEq, Eq)]
pub struct CombatResult {
    pub damage: i32,
    pub target_killed: bool,
}

/// Calculate melee damage: max(1, ATK - DEF + terrain_bonus).
pub fn calc_melee_damage(attacker: &Unit, defender: &Unit, grid: &Grid) -> i32 {
    let (dx, dy) = defender.grid_cell();
    let terrain_def = grid.get(dx, dy).defense_bonus();
    (attacker.stats.atk - defender.stats.def - terrain_def).max(1)
}

/// Calculate ranged damage: max(1, ATK - DEF + terrain_bonus).
pub fn calc_ranged_damage(attacker: &Unit, defender: &Unit, grid: &Grid) -> i32 {
    let (dx, dy) = defender.grid_cell();
    let terrain_def = grid.get(dx, dy).defense_bonus();
    (attacker.stats.atk - defender.stats.def - terrain_def).max(1)
}

/// Execute a melee attack. Mutates both units. Returns combat result.
pub fn execute_melee(attacker: &mut Unit, defender: &mut Unit, grid: &Grid) -> CombatResult {
    let damage = calc_melee_damage(attacker, defender, grid);
    defender.take_damage(damage);
    attacker.start_attack_cooldown();
    let anim = attacker.next_attack_anim();
    attacker.set_anim(anim);

    CombatResult {
        damage,
        target_killed: !defender.alive,
    }
}

/// Execute a heal: restore HP to an ally. Returns amount healed.
pub fn execute_heal(healer: &mut Unit, target: &mut Unit, base_heal: i32) -> i32 {
    let heal_amount = base_heal.min(target.stats.max_hp - target.hp);
    target.hp += heal_amount;
    healer.start_attack_cooldown();
    let anim = healer.next_attack_anim();
    healer.set_anim(anim);
    heal_amount
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, GRID_SIZE};
    use crate::unit::{Faction, UnitKind};

    fn make_warrior(id: u32, faction: Faction, x: u32, y: u32) -> Unit {
        Unit::new(id, UnitKind::Warrior, faction, x, y, false)
    }

    fn make_archer(id: u32, faction: Faction, x: u32, y: u32) -> Unit {
        Unit::new(id, UnitKind::Archer, faction, x, y, false)
    }

    #[test]
    fn melee_damage_calculation() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let attacker = make_warrior(1, Faction::Blue, 5, 5);
        let defender = make_warrior(2, Faction::Red, 6, 5);
        // ATK 3 - DEF 3 = 0, but min 1
        assert_eq!(calc_melee_damage(&attacker, &defender, &grid), 1);
    }

    #[test]
    fn melee_damage_vs_archer() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let attacker = make_warrior(1, Faction::Blue, 5, 5);
        let defender = make_archer(2, Faction::Red, 6, 5);
        // ATK 3 - DEF 1 = 2
        assert_eq!(calc_melee_damage(&attacker, &defender, &grid), 2);
    }

    #[test]
    fn terrain_defense_reduces_damage() {
        let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        grid.set(6, 5, crate::grid::TileKind::Forest);
        let attacker = make_warrior(1, Faction::Blue, 5, 5);
        let defender = make_archer(2, Faction::Red, 6, 5);
        // ATK 3 - DEF 1 - terrain 1 = 1
        assert_eq!(calc_melee_damage(&attacker, &defender, &grid), 1);
    }

    #[test]
    fn execute_melee_kills_low_hp() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut attacker = make_warrior(1, Faction::Blue, 5, 5);
        let mut defender = make_archer(2, Faction::Red, 6, 5);
        defender.hp = 1;
        let result = execute_melee(&mut attacker, &mut defender, &grid);
        assert!(result.target_killed);
        assert!(!defender.alive);
    }
}

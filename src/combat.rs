use crate::grid::Grid;
use crate::unit::{Facing, Unit, UnitAnim};

/// Result of a combat action.
#[derive(Debug, PartialEq, Eq)]
pub struct CombatResult {
    pub damage: i32,
    pub target_killed: bool,
}

/// Calculate melee damage: max(1, ATK - DEF + terrain_bonus).
pub fn calc_melee_damage(attacker: &Unit, defender: &Unit, grid: &Grid) -> i32 {
    let terrain_def = grid.get(defender.grid_x, defender.grid_y).defense_bonus();
    (attacker.stats.atk - defender.stats.def - terrain_def).max(1)
}

/// Calculate ranged damage: max(1, ATK - DEF + terrain_bonus).
pub fn calc_ranged_damage(attacker: &Unit, defender: &Unit, grid: &Grid) -> i32 {
    let terrain_def = grid.get(defender.grid_x, defender.grid_y).defense_bonus();
    (attacker.stats.atk - defender.stats.def - terrain_def).max(1)
}

/// Check if attacker can melee the defender (adjacent, range 1).
pub fn can_melee(attacker: &Unit, defender: &Unit) -> bool {
    attacker.alive
        && defender.alive
        && !attacker.has_attacked
        && attacker.faction != defender.faction
        && attacker.distance_to(defender.grid_x, defender.grid_y) <= 1
}

/// Check if attacker can shoot the defender (within range, for units with range > 1).
pub fn can_ranged_attack(attacker: &Unit, defender: &Unit) -> bool {
    attacker.alive
        && defender.alive
        && !attacker.has_attacked
        && attacker.faction != defender.faction
        && attacker.stats.range > 1
        && attacker.distance_to(defender.grid_x, defender.grid_y) <= attacker.stats.range
}

/// Execute a melee attack. Mutates both units. Returns combat result.
pub fn execute_melee(attacker: &mut Unit, defender: &mut Unit, grid: &Grid) -> CombatResult {
    let damage = calc_melee_damage(attacker, defender, grid);
    defender.take_damage(damage);
    attacker.has_attacked = true;

    // Face the defender
    if defender.grid_x > attacker.grid_x {
        attacker.facing = Facing::Right;
    } else if defender.grid_x < attacker.grid_x {
        attacker.facing = Facing::Left;
    }

    attacker.set_anim(UnitAnim::Attack);

    CombatResult {
        damage,
        target_killed: !defender.alive,
    }
}

/// Execute a ranged attack. Mutates both units. Returns combat result.
pub fn execute_ranged(attacker: &mut Unit, defender: &mut Unit, grid: &Grid) -> CombatResult {
    let damage = calc_ranged_damage(attacker, defender, grid);
    defender.take_damage(damage);
    attacker.has_attacked = true;

    if defender.grid_x > attacker.grid_x {
        attacker.facing = Facing::Right;
    } else if defender.grid_x < attacker.grid_x {
        attacker.facing = Facing::Left;
    }

    attacker.set_anim(UnitAnim::Attack);

    CombatResult {
        damage,
        target_killed: !defender.alive,
    }
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
        grid.set(6, 5, crate::grid::TileKind::Hill);
        let attacker = make_warrior(1, Faction::Blue, 5, 5);
        let defender = make_archer(2, Faction::Red, 6, 5);
        // ATK 3 - DEF 1 - terrain 1 = 1
        assert_eq!(calc_melee_damage(&attacker, &defender, &grid), 1);
    }

    #[test]
    fn can_melee_adjacent() {
        let a = make_warrior(1, Faction::Blue, 5, 5);
        let b = make_warrior(2, Faction::Red, 6, 5);
        assert!(can_melee(&a, &b));
    }

    #[test]
    fn cannot_melee_same_faction() {
        let a = make_warrior(1, Faction::Blue, 5, 5);
        let b = make_warrior(2, Faction::Blue, 6, 5);
        assert!(!can_melee(&a, &b));
    }

    #[test]
    fn cannot_melee_distant() {
        let a = make_warrior(1, Faction::Blue, 5, 5);
        let b = make_warrior(2, Faction::Red, 8, 5);
        assert!(!can_melee(&a, &b));
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

    #[test]
    fn can_ranged_attack_in_range() {
        let a = make_archer(1, Faction::Blue, 5, 5);
        let b = make_warrior(2, Faction::Red, 10, 5);
        assert!(can_ranged_attack(&a, &b));
    }

    #[test]
    fn cannot_ranged_attack_out_of_range() {
        let a = make_archer(1, Faction::Blue, 5, 5);
        let b = make_warrior(2, Faction::Red, 11, 5);
        assert!(!can_ranged_attack(&a, &b));
    }

    #[test]
    fn warrior_cannot_ranged_attack() {
        let a = make_warrior(1, Faction::Blue, 5, 5);
        let b = make_warrior(2, Faction::Red, 6, 5);
        assert!(!can_ranged_attack(&a, &b));
    }
}

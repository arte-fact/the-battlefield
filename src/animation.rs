use crate::unit::UnitId;

/// Events recorded during game logic execution for animation playback.
#[derive(Clone, Debug)]
pub enum TurnEvent {
    Move {
        unit_id: UnitId,
        from: (u32, u32),
        to: (u32, u32),
    },
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
    },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
        target_pos: (u32, u32),
        missed: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_event_move_creation() {
        let event = TurnEvent::Move {
            unit_id: 1,
            from: (5, 5),
            to: (6, 5),
        };
        match event {
            TurnEvent::Move {
                unit_id,
                from,
                to,
            } => {
                assert_eq!(unit_id, 1);
                assert_eq!(from, (5, 5));
                assert_eq!(to, (6, 5));
            }
            _ => panic!("wrong variant"),
        }
    }
}

/// Phases of the turn-based game loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnPhase {
    /// Player chooses their action.
    PlayerTurn,
    /// AI units act.
    AiTurn,
    /// Combat results are resolved, dead units removed.
    Resolution,
}

pub struct TurnState {
    pub phase: TurnPhase,
    pub turn_number: u32,
}

impl Default for TurnState {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnState {
    pub fn new() -> Self {
        Self {
            phase: TurnPhase::PlayerTurn,
            turn_number: 1,
        }
    }

    /// Advance to the next phase. Returns the new phase.
    pub fn advance(&mut self) -> TurnPhase {
        self.phase = match self.phase {
            TurnPhase::PlayerTurn => TurnPhase::AiTurn,
            TurnPhase::AiTurn => TurnPhase::Resolution,
            TurnPhase::Resolution => {
                self.turn_number += 1;
                TurnPhase::PlayerTurn
            }
        };
        self.phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let state = TurnState::new();
        assert_eq!(state.phase, TurnPhase::PlayerTurn);
        assert_eq!(state.turn_number, 1);
    }

    #[test]
    fn advance_cycle() {
        let mut state = TurnState::new();
        assert_eq!(state.advance(), TurnPhase::AiTurn);
        assert_eq!(state.advance(), TurnPhase::Resolution);
        assert_eq!(state.advance(), TurnPhase::PlayerTurn);
        assert_eq!(state.turn_number, 2);
    }

    #[test]
    fn three_full_cycles() {
        let mut state = TurnState::new();
        for _ in 0..9 {
            state.advance();
        }
        assert_eq!(state.turn_number, 4);
        assert_eq!(state.phase, TurnPhase::PlayerTurn);
    }
}

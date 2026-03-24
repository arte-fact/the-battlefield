/// Platform-agnostic player input for one game tick.
/// The wasm client converts browser events into this.
/// A multiplayer server would decode it from a binary WebSocket message.
#[derive(Clone, Debug, Default)]
pub struct PlayerInput {
    /// Movement direction (-1.0 to 1.0 per axis, zero = idle).
    pub move_x: f32,
    pub move_y: f32,
    /// True = attack this frame.
    pub attack: bool,
    /// Aim direction in radians (0 = right).
    pub aim_dir: f32,
    /// Whether the aim-lock modifier is held (Ctrl on keyboard, left trigger on gamepad).
    /// When true, aim direction and facing are locked regardless of movement.
    pub aim_lock: bool,
    /// Order commands (at most one per frame).
    pub order_hold: bool,
    pub order_go: bool,
    pub order_retreat: bool,
    pub order_follow: bool,
}

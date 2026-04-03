macro_rules! embed {
    ($path:literal) => {
        include_bytes!(concat!("../../../", $path))
    };
}

pub fn get(path: &str) -> Option<&'static [u8]> {
    match path {
        // Font
        "assets/MedievalSharp.ttf" => Some(embed!("assets/MedievalSharp.ttf")),
        "assets/Tilemap_road.png" => Some(embed!("assets/Tilemap_road.png")),

        // ── Blue Units ──────────────────────────────────────────────
        // Warrior
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Attack1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Attack1.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Attack2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Warrior/Warrior_Attack2.png")),
        // Archer
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Archer_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Archer_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Archer_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Archer_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Archer_Shoot.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Archer_Shoot.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Arrow.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Archer/Arrow.png")),
        // Lancer
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Lancer/Lancer_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Lancer/Lancer_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Lancer/Lancer_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Lancer/Lancer_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Lancer/Lancer_Right_Attack.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Lancer/Lancer_Right_Attack.png")),
        // Monk
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Heal.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Heal.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Heal_Effect.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Monk/Heal_Effect.png")),
        // Pawn
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Interact Axe.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Interact Axe.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Idle Wood.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Idle Wood.png")),
        "assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Run Wood.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Blue Units/Pawn/Pawn_Run Wood.png")),

        // ── Red Units ───────────────────────────────────────────────
        // Warrior
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Attack1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Attack1.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Attack2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Warrior/Warrior_Attack2.png")),
        // Archer
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Archer_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Archer_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Archer_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Archer_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Archer_Shoot.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Archer_Shoot.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Arrow.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Archer/Arrow.png")),
        // Lancer
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Lancer/Lancer_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Lancer/Lancer_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Lancer/Lancer_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Lancer/Lancer_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Lancer/Lancer_Right_Attack.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Lancer/Lancer_Right_Attack.png")),
        // Monk
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Monk/Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Monk/Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Monk/Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Monk/Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Monk/Heal.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Monk/Heal.png")),
        // Pawn
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Run.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Run.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Interact Axe.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Interact Axe.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Idle Wood.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Idle Wood.png")),
        "assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Run Wood.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Units/Red Units/Pawn/Pawn_Run Wood.png")),

        // ── Particles ───────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Particle FX/Dust_01.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Particle FX/Dust_01.png")),
        "assets/Tiny Swords (Free Pack)/Particle FX/Explosion_02.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Particle FX/Explosion_02.png")),

        // ── Buildings — Blue ────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Barracks.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Barracks.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Archery.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Archery.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Monastery.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Monastery.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Castle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Castle.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Tower.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/Tower.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/House1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/House1.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/House2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/House2.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/House3.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Blue Buildings/House3.png")),

        // ── Buildings — Red ─────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Barracks.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Barracks.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Archery.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Archery.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Monastery.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Monastery.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Castle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Castle.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Tower.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/Tower.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/House1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/House1.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/House2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/House2.png")),
        "assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/House3.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Red Buildings/House3.png")),

        // ── Buildings — Black (Tower only) ──────────────────────────
        "assets/Tiny Swords (Free Pack)/Buildings/Black Buildings/Tower.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Buildings/Black Buildings/Tower.png")),

        // ── Terrain — Tileset ───────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Terrain/Tileset/Tilemap_color1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Tileset/Tilemap_color1.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Tileset/Tilemap_color2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Tileset/Tilemap_color2.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Tileset/Water Background color.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Tileset/Water Background color.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Tileset/Water Foam.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Tileset/Water Foam.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Tileset/Shadow.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Tileset/Shadow.png")),

        // ── Trees ───────────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree1.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree2.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree3.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree3.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree4.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Wood/Trees/Tree4.png")),

        // ── Bushes ──────────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe1.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe2.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe3.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe3.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe4.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Bushes/Bushe4.png")),

        // ── Rocks ───────────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock1.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock1.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock2.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock2.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock3.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock3.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock4.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks/Rock4.png")),

        // ── Water Rocks ─────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_01.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_01.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_02.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_02.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_03.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_03.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_04.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Decorations/Rocks in the Water/Water Rocks_04.png")),

        // ── UI ──────────────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Papers/SpecialPaper.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Papers/SpecialPaper.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Buttons/BigBlueButton_Regular.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Buttons/BigBlueButton_Regular.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Buttons/BigRedButton_Regular.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Buttons/BigRedButton_Regular.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Bars/BigBar_Base.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Bars/BigBar_Base.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Bars/BigBar_Fill.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Bars/BigBar_Fill.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Ribbons/BigRibbons.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Ribbons/BigRibbons.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Ribbons/SmallRibbons.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Ribbons/SmallRibbons.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Swords/Swords.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Swords/Swords.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Wood Table/WoodTable.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Wood Table/WoodTable.png")),

        // ── Avatars ─────────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_01.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_01.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_02.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_02.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_03.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_03.png")),
        "assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_04.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/UI Elements/UI Elements/Human Avatars/Avatars_04.png")),

        // ── Sheep ───────────────────────────────────────────────────
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Meat/Sheep/Sheep_Idle.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Meat/Sheep/Sheep_Idle.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Meat/Sheep/Sheep_Move.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Meat/Sheep/Sheep_Move.png")),
        "assets/Tiny Swords (Free Pack)/Terrain/Resources/Meat/Sheep/Sheep_Grass.png" =>
            Some(embed!("assets/Tiny Swords (Free Pack)/Terrain/Resources/Meat/Sheep/Sheep_Grass.png")),

        _ => None,
    }
}

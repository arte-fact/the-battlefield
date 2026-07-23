use super::*;

impl Game {
    /// Wave composition — 21 units interleaved so all buildings produce continuously.
    /// 7 Warriors, 4 Lancers, 8 Archers, 2 Monks.
    const WAVE: &'static [UnitKind] = &[
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Monk,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Monk,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Archer,
    ];

    pub fn spawn_unit(
        &mut self,
        kind: UnitKind,
        faction: Faction,
        x: u32,
        y: u32,
        is_player: bool,
    ) -> UnitId {
        // Find nearest passable tile via spiral search if requested tile is blocked
        let (sx, sy) = if self.grid.in_bounds(x as i32, y as i32) && self.grid.is_passable(x, y) {
            (x, y)
        } else {
            self.find_nearest_passable(x, y).unwrap_or((x, y))
        };

        let id = self.next_unit_id;
        self.next_unit_id += 1;
        let mut unit = Unit::new(id, kind, faction, sx, sy, is_player);
        // Apply runtime combat stats from config
        unit.stats = kind.stats_from_config(&self.config);
        unit.hp = unit.stats.max_hp;
        // Stagger AI initial attack cooldowns to prevent all acting on the same frame
        if !is_player {
            unit.attack_cooldown = (id as f32 * 0.05) % 0.3;
        }
        self.units.push(unit);
        id
    }

    /// Spiral search for the nearest passable tile around (cx, cy).
    pub(super) fn find_nearest_passable(&self, cx: u32, cy: u32) -> Option<(u32, u32)> {
        for radius in 1..16i32 {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue; // only check the ring perimeter
                    }
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if self.grid.in_bounds(nx, ny) && self.grid.is_passable(nx as u32, ny as u32) {
                        return Some((nx as u32, ny as u32));
                    }
                }
            }
        }
        None
    }

    /// Tick capture zone progress based on unit positions.
    pub fn tick_zones(&mut self, dt: f32) {
        // Snapshot per-zone state before tick
        let states_before: Vec<ZoneState> =
            self.zone_manager.zones.iter().map(|z| z.state).collect();

        self.zone_manager.count_units(&self.units);
        self.zone_manager.tick_capture(
            dt,
            self.config.base_capture_time,
            self.config.max_capture_multiplier,
        );

        // Collect zone state changes, then apply reputation
        let zone_changes: Vec<_> = self
            .zone_manager
            .zones
            .iter()
            .enumerate()
            .filter_map(|(i, zone)| {
                let before = states_before[i];
                let after = zone.state;
                if before == after {
                    return None;
                }
                let in_fov = self.is_tile_in_fov(zone.center_gx, zone.center_gy);
                Some((
                    i as u8,
                    before,
                    after,
                    in_fov,
                    zone.center_wx,
                    zone.center_wy,
                ))
            })
            .collect();

        for (zi, before, after, in_fov, zx, zy) in zone_changes {
            // The village serves its new lord: surviving garrison converts.
            if let ZoneState::Controlled(f) = after {
                self.convert_garrison(zi, f);
            }
            let army = self.player_army();
            if before != ZoneState::Controlled(army)
                && after == ZoneState::Controlled(army)
            {
                self.on_zone_captured(in_fov, zx, zy);
            }
            if before == ZoneState::Controlled(army)
                && after != ZoneState::Controlled(army)
            {
                self.on_zone_lost(in_fov, zx, zy);
            }
            if let ZoneState::Controlled(rival) = before {
                if rival != army && after != before {
                    self.on_zone_decaptured(in_fov, zx, zy);
                }
            }
        }

        self.tick_starvation(dt);
        self.check_annihilation();
    }

    /// A landless army has no supply line: after a short grace its units
    /// wither, so remnants can't stall a decided war forever. Retaking
    /// any settlement resets the clock.
    fn tick_starvation(&mut self, dt: f32) {
        const STARVATION_GRACE: f32 = 20.0;
        const STARVATION_TICK: f32 = 3.0;
        for &f in self.active_factions() {
            let fi = f.idx();
            if self.zone_manager.controlled_count(f) > 0 {
                self.starvation[fi] = 0.0;
                self.starving[fi] = false;
                continue;
            }
            let before = self.starvation[fi];
            self.starvation[fi] += dt;
            let after = self.starvation[fi];
            if after < STARVATION_GRACE {
                continue;
            }
            self.starving[fi] = true;
            let ticks_before = ((before - STARVATION_GRACE).max(0.0) / STARVATION_TICK) as i32;
            let ticks_after = ((after - STARVATION_GRACE) / STARVATION_TICK) as i32;
            let hits = ticks_after - ticks_before;
            if hits <= 0 {
                continue;
            }
            for u in &mut self.units {
                if u.alive && u.faction == f {
                    u.take_damage(hits);
                }
            }
        }
    }

    /// Conquest bleed: controlling a majority of zones drains the enemy pool,
    /// scaling with each zone at or above the threshold.
    /// Conquest defeat: a faction is eliminated when it owns no
    /// settlements and has no living units — nothing left to fight or
    /// train with. Last banner standing wins.
    fn check_annihilation(&mut self) {
        if self.winner.is_some() {
            return;
        }
        let active = self.active_factions();
        let standing: Vec<Faction> = active
            .iter()
            .copied()
            .filter(|&f| {
                self.zone_manager.controlled_count(f) > 0
                    || self.units.iter().any(|u| u.alive && u.faction == f)
            })
            .collect();
        if active.len() > 1 && standing.len() == 1 {
            self.winner = Some(standing[0]);
        } else if active.len() > 1
            && self.player_faction.is_some()
            && !standing.contains(&self.player_army())
        {
            // The player's faction is annihilated while rivals still fight:
            // the run ends now — credit the current settlement leader.
            let leader = standing
                .iter()
                .copied()
                .max_by_key(|&f| self.zone_manager.controlled_count(f))
                .unwrap_or(Faction::Red);
            self.winner = Some(leader);
        }
    }

    /// The zone a faction rallies and reinforces at: its largest
    /// controlled settlement (tier, then lowest id). None when landless.
    pub fn rally_zone(&self, faction: Faction) -> Option<&crate::zone::CaptureZone> {
        self.zone_manager
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction))
            .max_by_key(|z| (z.tier, std::cmp::Reverse(z.id)))
    }

    /// The pawn economy: houses raise recruits (1 meat), recruits walk
    /// to a production building and convert into its soldier — gold for
    /// warriors and monks, wood for archers and lancers. Owned and
    /// neutral settlements run the same rule; free villages draw on the
    /// shared Villager pools and their converts stay home as militia.
    pub fn tick_recruits(&mut self, dt: f32) {
        let interval = (self.config.levy_interval * self.config.train_speed_mult).max(1.0);
        if self.recruit_rr.len() != self.zone_manager.zones.len() {
            self.recruit_rr = vec![0; self.zone_manager.zones.len()];
        }

        // Live counts once per tick.
        let mut army_units = [0usize; 4];
        for u in &self.units {
            if u.alive && !u.is_player {
                if let Some(i) = u.faction.army_idx() {
                    army_units[i] += 1;
                }
            }
        }
        let mut inflight = [0usize; 5];
        let mut inflight_zone = vec![0usize; self.zone_manager.zones.len()];
        for p in &self.pawns {
            if let Some(t) = &p.recruit {
                inflight[p.faction.pool_idx()] += 1;
                if let Some(c) = inflight_zone.get_mut(t.zone as usize) {
                    *c += 1;
                }
            }
        }

        let mut pawn_seed = 0x5EED_u32;
        for bi in 0..self.buildings.len() {
            if self.buildings[bi].train_cooldown > 0.0 {
                self.buildings[bi].train_cooldown -= dt;
                continue;
            }
            let b = &self.buildings[bi];
            if b.kind != building::BuildingKind::House {
                continue;
            }
            let Some(zid) = b.zone_id else { continue };
            let zi = zid as usize;
            let Some(zone) = self.zone_manager.zones.get(zi) else {
                continue;
            };
            let faction = match zone.state {
                ZoneState::Controlled(f) => f,
                ZoneState::Neutral => Faction::Villager,
                _ => continue, // embattled settlements raise no one
            };
            let pool = faction.pool_idx();
            if self.resources[pool][crate::pawn::ResourceKind::Meat.idx()] == 0 {
                continue;
            }

            // Headroom: armies against the cap (walking recruits count),
            // militia against its settlement's tier cap.
            match faction.army_idx() {
                Some(fi) => {
                    if army_units[fi] + inflight[pool] >= self.config.max_units_per_faction {
                        continue;
                    }
                }
                None => {
                    let local = self
                        .units
                        .iter()
                        .filter(|u| {
                            u.alive
                                && u.faction == Faction::Villager
                                && u.order
                                    == Some(crate::unit::OrderKind::DefendZone { zone: zid })
                        })
                        .count();
                    if local + inflight_zone[zi] >= zone.tier.garrison_cap() as usize {
                        continue;
                    }
                }
            }

            // No ghost-town crowds: bound walking recruits per settlement.
            let prod: Vec<(usize, UnitKind)> = self
                .buildings
                .iter()
                .enumerate()
                .filter(|(_, pb)| pb.zone_id == Some(zid))
                .filter_map(|(i, pb)| pb.produces.map(|k| (i, k)))
                .collect();
            if prod.is_empty() || inflight_zone[zi] >= prod.len() * 2 {
                continue;
            }

            // Round-robin over the buildings whose typed cost is payable.
            let affordable: Vec<(usize, UnitKind)> = prod
                .iter()
                .copied()
                .filter(|&(_, k)| self.resources[pool][k.conversion_cost().idx()] > 0)
                .collect();
            let Some(&(target_bi, _kind)) = (!affordable.is_empty())
                .then(|| {
                    let pick = self.recruit_rr[zi] as usize % affordable.len();
                    &affordable[pick]
                })
                .or(None)
            else {
                continue;
            };
            self.recruit_rr[zi] = self.recruit_rr[zi].wrapping_add(1);

            // Raise the recruit.
            self.resources[pool][crate::pawn::ResourceKind::Meat.idx()] -= 1;
            let (hx, hy) = grid::grid_to_world(self.buildings[bi].grid_x, self.buildings[bi].grid_y);
            let tb = &self.buildings[target_bi];
            let (tx, ty) = grid::grid_to_world(tb.grid_x, tb.grid_y);
            pawn_seed = pawn_seed
                .wrapping_mul(1103515245)
                .wrapping_add(12345 + bi as u32);
            self.pawns.push(crate::pawn::Pawn::new_recruit(
                hx,
                hy,
                faction,
                crate::pawn::RecruitTask {
                    zone: zid,
                    building: target_bi,
                    target: (tx, ty),
                    arrived: false,
                    wait: 0.0,
                },
                pawn_seed,
            ));
            inflight[pool] += 1;
            inflight_zone[zi] += 1;
            self.buildings[bi].train_cooldown = interval;
        }
    }

    /// Arrived recruits enlist: spend the typed cost and swap the pawn
    /// for a soldier at the door. Called from the pawn tick.
    pub(crate) fn process_recruit_arrivals(&mut self, pawns: &mut Vec<crate::pawn::Pawn>, dt: f32) {
        const RETARGET_AFTER: f32 = 15.0;
        let mut i = 0;
        while i < pawns.len() {
            let Some(task) = pawns[i].recruit.clone() else {
                i += 1;
                continue;
            };
            let zid = task.zone;
            let faction = pawns[i].faction;
            let zone_ok = self
                .zone_manager
                .zones
                .get(zid as usize)
                .map(|z| match z.state {
                    ZoneState::Controlled(f) => f == faction,
                    ZoneState::Neutral => faction == Faction::Villager,
                    _ => false,
                })
                .unwrap_or(false);
            if !zone_ok {
                // The settlement fell: the levy melts away.
                pawns.swap_remove(i);
                continue;
            }
            if !task.arrived {
                i += 1;
                continue;
            }

            let kind = self.buildings[task.building]
                .produces
                .expect("recruit targets are production buildings");
            let pool = faction.pool_idx();
            let cost = kind.conversion_cost().idx();

            // Militia respects its tier cap even at the door.
            let militia_blocked = faction == Faction::Villager && {
                let cap = self.zone_manager.zones[zid as usize].tier.garrison_cap() as usize;
                let local = self
                    .units
                    .iter()
                    .filter(|u| {
                        u.alive
                            && u.faction == Faction::Villager
                            && u.order == Some(crate::unit::OrderKind::DefendZone { zone: zid })
                    })
                    .count();
                local >= cap
            };

            if self.resources[pool][cost] > 0 && !militia_blocked {
                self.resources[pool][cost] -= 1;
                self.floating_texts.push(super::FloatingText {
                    x: pawns[i].x,
                    y: pawns[i].y - 40.0,
                    value: -1.0,
                    remaining: 1.0,
                    kind: super::FloatKind::Resource(kind.conversion_cost()),
                });
                let (gx, gy) = grid::world_to_grid(pawns[i].x, pawns[i].y);
                let id = self.spawn_unit(kind, faction, gx.max(0) as u32, gy.max(0) as u32, false);
                if faction == Faction::Villager {
                    if let Some(u) = self.units.iter_mut().find(|u| u.id == id) {
                        u.order = Some(crate::unit::OrderKind::DefendZone { zone: zid });
                        u.order_timer = 0.0;
                    }
                }
                pawns.swap_remove(i);
                continue;
            }

            // Can't pay: wait at the door, then try another line of work.
            if let Some(t) = pawns[i].recruit.as_mut() {
                t.wait += dt;
            }
            if pawns[i].recruit.as_ref().is_some_and(|t| t.wait >= RETARGET_AFTER) {
                let affordable: Vec<(usize, UnitKind)> = self
                    .buildings
                    .iter()
                    .enumerate()
                    .filter(|(i2, pb)| pb.zone_id == Some(zid) && *i2 != task.building)
                    .filter_map(|(i2, pb)| pb.produces.map(|k| (i2, k)))
                    .filter(|&(_, k)| self.resources[pool][k.conversion_cost().idx()] > 0)
                    .collect();
                if let Some(&(nbi, _)) = affordable.first() {
                    let tb = &self.buildings[nbi];
                    let (tx, ty) = grid::grid_to_world(tb.grid_x, tb.grid_y);
                    pawns[i].retarget_recruit(crate::pawn::RecruitTask {
                        zone: zid,
                        building: nbi,
                        target: (tx, ty),
                        arrived: false,
                        wait: 0.0,
                    });
                } else if let Some(t) = pawns[i].recruit.as_mut() {
                    t.wait = 0.0; // nothing affordable anywhere: keep waiting
                }
            }
            i += 1;
        }
    }

    /// Surviving neutral militia pledges to the captor when its village
    /// falls. Army garrisons (stationed or village-produced in a faction
    /// color) never defect.
    pub(super) fn convert_garrison(&mut self, zone_idx: u8, new_owner: Faction) {
        for u in &mut self.units {
            if u.alive
                && u.faction == Faction::Villager
                && u.order == Some(crate::unit::OrderKind::DefendZone { zone: zone_idx })
                && u.faction != new_owner
            {
                u.faction = new_owner;
                u.combat_target = None;
                u.hit_flash = 0.0;
            }
        }
    }

    pub fn setup_demo_battle_with_seed(&mut self, seed: u32) {
        let (gen_grid, layout) =
            mapgen::generate_battlefield_n(seed, self.config.playable_size, self.n_capitals());
        self.finish_setup(seed, gen_grid, layout);
    }

    fn n_capitals(&self) -> u32 {
        2 + self.config.enemy_count.clamp(1, 3).saturating_sub(1) as u32
    }

    /// Start budgeted map generation; hosts pump it with `setup_step`
    /// while showing the loading screen.
    pub fn begin_async_setup(&mut self, seed: u32) {
        self.pending_setup = Some(mapgen::MapGen::new(
            seed,
            self.config.playable_size,
            self.n_capitals(),
        ));
    }

    /// Advance pending generation one bounded chunk. Returns true while
    /// loading; the finishing call completes battle setup before returning
    /// false.
    pub fn setup_step(&mut self) -> bool {
        let Some(job) = self.pending_setup.as_mut() else {
            return false;
        };
        if !job.step() {
            return true;
        }
        let job = self.pending_setup.take().expect("checked above");
        let seed = job.seed();
        let (gen_grid, layout) = job.take_result();
        self.finish_setup(seed, gen_grid, layout);
        false
    }

    pub fn setup_progress(&self) -> f32 {
        self.pending_setup
            .as_ref()
            .map(|j| j.progress())
            .unwrap_or(1.0)
    }

    fn finish_setup(&mut self, seed: u32, gen_grid: Grid, layout: mapgen::MapLayout) {
        self.grid = gen_grid;
        self.fog_generation = self.fog_generation.wrapping_add(1);
        let tiles = (self.grid.width * self.grid.height) as usize;
        self.visible = vec![false; tiles];
        self.revealed = vec![true; tiles];

        let (blue_cx, blue_cy) = layout.blue_base;
        let (red_cx, red_cy) = layout.red_base;

        // Per-faction base positions: Blue, Red, then provisional capitals.
        let mut base_pos: [(u32, u32); 4] = [layout.blue_base, layout.red_base, (0, 0), (0, 0)];
        for (i, &b) in layout.extra_bases.iter().enumerate() {
            base_pos[2 + i] = b;
        }
        let active: Vec<Faction> = self.active_factions().to_vec();

        // Fallback objective: the map centre — real targets come from the
        // zone planner within the first refresh.
        let centre = grid::grid_to_world(self.grid.width / 2, self.grid.height / 2);
        self.faction_objectives = [centre; 4];

        // Create capture zones from BSP layout
        self.zone_manager = ZoneManager::create_from_layout(&layout, self.config.zone_radius);
        // Fresh war chests (config may have been live-tuned since Game::new).
        self.resources = [[5, 3, 3]; 5];
        self.recruit_rr = vec![0; self.zone_manager.zones.len()];

        // Capitals are the first zones (ids 0..n_capitals) in
        // [Blue, Red, extras...] order; each active faction starts in
        // control of its own.
        let capital_zone = |k: usize| k as u8;
        for (k, &f) in active.iter().enumerate() {
            let zi = capital_zone(k) as usize;
            if zi < self.zone_manager.zones.len() {
                self.zone_manager.zones[zi].set_controlled(f);
            }
        }

        // Store gather points for unit rallying
        self.gathers = base_pos;

        // Bases face each other; every band rotates with this vector.
        let dxf = red_cx as f32 - blue_cx as f32;
        let dyf = red_cy as f32 - blue_cy as f32;
        let len = (dxf * dxf + dyf * dyf).sqrt().max(1.0);
        let blue_facing = (dxf / len, dyf / len);
        let red_facing = (-blue_facing.0, -blue_facing.1);

        // Procedurally generate capital buildings for every active
        // faction — zone-linked so they recolor with whoever holds the
        // city; extra capitals face the map centre.
        let mut buildings = Vec::new();
        for (k, &f) in active.iter().enumerate() {
            let (cx, cy) = base_pos[f.idx()];
            let facing = match f {
                Faction::Blue => blue_facing,
                Faction::Red => red_facing,
                _ => {
                    let dxf = self.grid.width as f32 / 2.0 - cx as f32;
                    let dyf = self.grid.height as f32 / 2.0 - cy as f32;
                    let len = (dxf * dxf + dyf * dyf).sqrt().max(1.0);
                    (dxf / len, dyf / len)
                }
            };
            let mut batch = building::generate_base_buildings(
                f,
                cx,
                cy,
                seed.wrapping_add(0x1000 * f.idx() as u32),
                facing,
                self.config.playable_size,
            );
            for b in &mut batch {
                b.zone_id = Some(capital_zone(k));
            }
            buildings.extend(batch);
        }
        // Place defense towers at capture zone centers
        for zone in &self.zone_manager.zones {
            buildings.push(building::BaseBuilding {
                kind: building::BuildingKind::DefenseTower,
                faction: Faction::Blue, // updated dynamically based on zone state
                grid_x: zone.center_gx,
                grid_y: zone.center_gy,
                attack_cooldown: 0.0,
                zone_id: Some(zone.id),
                produces: None,
                house_variant: 0,
                train_cooldown: 0.0,
            });
        }
        // Village buildings from the map plan; ownership follows the zone.
        for v in &layout.settlements {
            for (i, &(hx, hy)) in v.houses.iter().enumerate() {
                buildings.push(building::BaseBuilding {
                    kind: building::BuildingKind::House,
                    faction: Faction::Blue, // rendered/owned via zone state
                    grid_x: hx,
                    grid_y: hy,
                    attack_cooldown: 0.0,
                    zone_id: Some(v.zone_idx),
                    produces: None,
                    house_variant: (i % 3) as u8,
                    train_cooldown: 0.0,
                });
            }
            for &((bx, by), kind) in &v.production {
                let unit = match kind {
                    building::BuildingKind::Barracks => UnitKind::Warrior,
                    building::BuildingKind::Archery => UnitKind::Archer,
                    building::BuildingKind::Monastery => UnitKind::Monk,
                    _ => continue,
                };
                buildings.push(building::BaseBuilding {
                    kind,
                    faction: Faction::Blue, // rendered/owned via zone state
                    grid_x: bx,
                    grid_y: by,
                    attack_cooldown: 0.0,
                    zone_id: Some(v.zone_idx),
                    produces: Some(unit),
                    house_variant: 0,
                    train_cooldown: 0.0,
                });
            }
        }
        for b in &buildings {
            for &(dx, dy) in b.kind.footprint_offsets() {
                let fx = b.grid_x as i32 + dx;
                let fy = b.grid_y as i32 + dy;
                if self.grid.in_bounds(fx, fy) {
                    self.grid.mark_building(fx as u32, fy as u32);
                }
            }
        }
        // Paint road/dirt around building footprints, sparing village
        // resources (groves, gold stones, pen ground).
        let protected: std::collections::HashSet<(u32, u32)> = layout
            .settlements
            .iter()
            .flat_map(|v| v.resources.iter().copied())
            .collect();
        Self::paint_road_around_buildings(&mut self.grid, &buildings, &protected);
        // Stagger house timers so an empire doesn't raise recruits in
        // one synchronized burst.
        let interval = (self.config.levy_interval * self.config.train_speed_mult).max(1.0);
        for (i, b) in buildings.iter_mut().enumerate() {
            if b.kind == building::BuildingKind::House {
                b.train_cooldown = interval * ((i % 8) as f32 / 8.0);
            }
        }
        self.buildings = buildings;

        // Spawn ambient sheep in rear pasture of each base
        self.spawn_base_sheep(blue_cx, blue_cy, blue_facing, seed);
        self.spawn_base_sheep(red_cx, red_cy, red_facing, seed.wrapping_add(7919));

        // Village resources per tile: gold outcrops as impassable
        // decorations, pasture sheep grazing pen tiles (groves are
        // already terrain). Capitals mix all three in arcs.
        for v in &layout.settlements {
            let mut sheep_seed = seed
                .wrapping_mul(0x0065_B0A5)
                .wrapping_add(v.zone_idx as u32);
            for (&(x, y), &theme) in v.resources.iter().zip(v.resource_themes.iter()) {
                match theme {
                    crate::mapgen::VillageTheme::Gold => {
                        let variant =
                            ((x.wrapping_mul(31).wrapping_add(y.wrapping_mul(17))) % 6) as u8;
                        self.grid
                            .set_decoration(x, y, Some(grid::Decoration::GoldStone(variant)));
                    }
                    crate::mapgen::VillageTheme::Meat => {
                        sheep_seed = sheep_seed.wrapping_mul(1103515245).wrapping_add(12345);
                        let (wx, wy) = grid::grid_to_world(x, y);
                        self.sheep.push(Sheep::new(wx, wy, sheep_seed));
                    }
                    crate::mapgen::VillageTheme::Wood => {}
                }
            }
        }

        // Spawn one pawn worker per house; village pawns work their
        // zone's resource and recolor with its owner. Mixed settlements
        // (capitals) split their workers across the themes present.
        let mut pawn_seed = seed.wrapping_add(0xCAFE);
        let mut house_no: std::collections::HashMap<u8, usize> = std::collections::HashMap::new();
        for b in &self.buildings {
            if b.kind != building::BuildingKind::House {
                continue;
            }
            let (wx, wy) = grid::grid_to_world(b.grid_x, b.grid_y);
            pawn_seed = pawn_seed.wrapping_mul(1103515245).wrapping_add(12345);
            match b.zone_id {
                None => self.pawns.push(Pawn::new(wx, wy, b.faction, pawn_seed)),
                Some(zid) => {
                    let Some(v) = layout.settlements.iter().find(|v| v.zone_idx == zid) else {
                        continue;
                    };
                    let mut themes: Vec<crate::mapgen::VillageTheme> = Vec::new();
                    for &t in &v.resource_themes {
                        if !themes.contains(&t) {
                            themes.push(t);
                        }
                    }
                    if themes.is_empty() {
                        themes.push(v.theme);
                    }
                    let idx = house_no.entry(zid).or_insert(0);
                    let theme = themes[*idx % themes.len()];
                    *idx += 1;
                    let (job, work_tiles) = match theme {
                        crate::mapgen::VillageTheme::Gold => {
                            (crate::pawn::PawnJob::Mine, Vec::new())
                        }
                        crate::mapgen::VillageTheme::Wood => {
                            (crate::pawn::PawnJob::Chop, Vec::new())
                        }
                        crate::mapgen::VillageTheme::Meat => {
                            let pen: Vec<(u32, u32)> = v
                                .resources
                                .iter()
                                .zip(v.resource_themes.iter())
                                .filter(|&(_, &t)| t == crate::mapgen::VillageTheme::Meat)
                                .map(|(&p, _)| p)
                                .collect();
                            (crate::pawn::PawnJob::Herd, pen)
                        }
                    };
                    self.pawns.push(Pawn::with_job(
                        wx,
                        wy,
                        b.faction,
                        job,
                        Some(zid),
                        work_tiles,
                        pawn_seed,
                    ));
                }
            }
        }

        // Spawn the player: enlisted runs start as a Warrior at their
        // army's capital; unaligned (free-mode) runs start as a villager
        // at the countryside settlement farthest from every capital.
        match self.player_faction {
            Some(army) => {
                let (pcx, pcy) = if army == Faction::Blue {
                    (blue_cx, blue_cy)
                } else {
                    base_pos[army.idx()]
                };
                self.spawn_unit(UnitKind::Warrior, army, pcx, pcy, true);
            }
            None => {
                let n_caps = active.len();
                let capitals: Vec<(f32, f32)> = self.zone_manager.zones[..n_caps]
                    .iter()
                    .map(|z| (z.center_wx, z.center_wy))
                    .collect();
                let (pcx, pcy) = self.zone_manager.zones[n_caps..]
                    .iter()
                    .max_by_key(|z| {
                        capitals
                            .iter()
                            .map(|&(cx, cy)| {
                                ((z.center_wx - cx).powi(2) + (z.center_wy - cy).powi(2)) as i64
                            })
                            .min()
                            .unwrap_or(0)
                    })
                    .map(|z| (z.center_gx, z.center_gy))
                    .unwrap_or((blue_cx, blue_cy));
                let id = self.spawn_unit(UnitKind::Warrior, Faction::Villager, pcx, pcy, true);
                if let Some(u) = self.units.iter_mut().find(|u| u.id == id) {
                    // A villager, not a soldier: fragile, unarmed in spirit.
                    u.stats.max_hp = 5;
                    u.hp = 5;
                }
            }
        }

        // Spawn starting armies around each base — same composition as a wave, no rally hold
        for &(faction, base_cx, base_cy) in active
            .iter()
            .map(|&f| {
                let (cx, cy) = base_pos[f.idx()];
                (f, cx, cy)
            })
            .collect::<Vec<_>>()
            .iter()
        {
            for (i, &kind) in Self::WAVE.iter().enumerate() {
                // Spread units in a grid around the base center
                let col = (i % 5) as u32;
                let row = (i / 5) as u32;
                let sx = base_cx.saturating_sub(2) + col;
                let sy = base_cy.saturating_sub(1) + row;
                self.spawn_unit(kind, faction, sx, sy, false);
            }
        }

        // Camera starts centered on player (base center)
        let (cx, cy) = grid::grid_to_world(blue_cx, blue_cy);
        self.camera.x = cx;
        self.camera.y = cy;
        self.camera.zoom = self.camera.ideal_zoom();

        // Pre-compute caches
        self.grid.recompute_caches();
        self.compute_water_adjacency();
        self.compute_fov();
    }

    /// Spawn 10 sheep at random positions in the rear pasture (behind the houses).
    /// `fs` = front sign: 1 for Blue (front=+Y), -1 for Red (front=-Y).
    /// Paint a 1-tile road border around the bounding box of each building footprint.
    fn paint_road_around_buildings(
        grid: &mut Grid,
        buildings: &[building::BaseBuilding],
        protected: &std::collections::HashSet<(u32, u32)>,
    ) {
        for b in buildings {
            // Compute bounding box of all footprint cells (absolute coords)
            let offsets = b.kind.footprint_offsets();
            if offsets.is_empty() {
                continue;
            }
            let mut min_x = i32::MAX;
            let mut max_x = i32::MIN;
            let mut min_y = i32::MAX;
            let mut max_y = i32::MIN;
            for &(dx, dy) in offsets {
                let ax = b.grid_x as i32 + dx;
                let ay = b.grid_y as i32 + dy;
                min_x = min_x.min(ax);
                max_x = max_x.max(ax);
                min_y = min_y.min(ay);
                max_y = max_y.max(ay);
            }
            // Expand by 2 tiles in all directions
            let border = 2;
            for ry in (min_y - border)..=(max_y + border) {
                for rx in (min_x - border)..=(max_x + border) {
                    if !grid.in_bounds(rx, ry) {
                        continue;
                    }
                    let ux = rx as u32;
                    let uy = ry as u32;
                    if protected.contains(&(ux, uy)) {
                        continue;
                    }
                    let tile = grid.get(ux, uy);
                    if (tile == TileKind::Grass || tile == TileKind::Forest)
                        && grid.elevation(ux, uy) == 0
                    {
                        grid.set(ux, uy, TileKind::Road);
                        grid.set_decoration(ux, uy, None);
                    }
                }
            }
        }
    }

    fn spawn_base_sheep(&mut self, cx: u32, cy: u32, facing: (f32, f32), base_seed: u32) {
        let mut seed = if base_seed == 0 { 1 } else { base_seed };
        let mut next = || -> u32 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let (fx, fy) = facing;
        let (px, py) = (-fy, fx);
        for _ in 0..10 {
            // Rear pasture: lateral ∈ [-4,4], 11-14 tiles behind the center.
            let lat = (next() % 9) as f32 - 4.0;
            let rear = (next() % 4) as f32 + 11.0;
            let gx = (cx as f32 - fx * rear + px * lat).round().max(0.0) as u32;
            let gy = (cy as f32 - fy * rear + py * lat).round().max(0.0) as u32;
            if !self.grid.is_passable(gx, gy) {
                continue;
            }
            let (wx, wy) = grid::grid_to_world(gx, gy);
            self.sheep.push(Sheep::new(wx, wy, next()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_zones_updates_capture_progress() {
        use crate::mapgen::MapLayout;
        let mut game = Game::new(960.0, 640.0);
        let layout = MapLayout {
            blue_base: (21, 21),
            red_base: (138, 138),
            zone_centers: vec![(50, 50), (80, 80), (110, 110)],
            blue_gather: (21, 21),
            red_gather: (138, 138),
            blue_home_zones: vec![0],
            red_home_zones: vec![2],
            connections: vec![vec![1], vec![0, 2], vec![1]],
            settlements: Vec::new(),
            extra_bases: Vec::new(),
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout, game.config.zone_radius);
        let z0gx = game.zone_manager.zones[0].center_gx;
        let z0gy = game.zone_manager.zones[0].center_gy;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, z0gx, z0gy, false);
        game.tick_zones(4.0);
        assert!(
            game.zone_manager.zones[0].progress > 0.0,
            "Zone progress should advance with a Blue unit inside"
        );
    }

    #[test]
    fn neutral_village_raises_villager_garrison() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        game.resources[4] = [30, 30, 30];
        for _ in 0..(10 * 120) {
            game.tick_recruits(0.1);
            game.tick_pawns(0.1);
        }
        let militia: Vec<_> = game
            .units
            .iter()
            .filter(|u| u.alive && u.faction == Faction::Villager)
            .collect();
        assert!(
            !militia.is_empty(),
            "neutral villages must raise Villager garrisons"
        );
        for u in &militia {
            assert!(
                matches!(u.order, Some(crate::unit::OrderKind::DefendZone { .. })),
                "garrison units must carry the DefendZone stance"
            );
        }
        // Cap respected per zone
        for zi in 0..game.zone_manager.zones.len() {
            let count = game
                .units
                .iter()
                .filter(|u| {
                    u.alive
                        && u.order == Some(crate::unit::OrderKind::DefendZone { zone: zi as u8 })
                })
                .count();
            let cap = game.zone_manager.zones[zi].tier.garrison_cap() as usize;
            assert!(count <= cap, "zone {zi} militia over tier cap: {count}");
        }
        assert!(
            game.resources[4][0] < 30,
            "raising the levy must consume the network's meat"
        );
    }

    #[test]
    fn captured_village_trains_owner_soldiers() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        game.zone_manager.zones[3].set_controlled(Faction::Blue);
        game.resources = [[0; 3]; 5];
        game.resources[0] = [20, 20, 20];
        for _ in 0..(10 * 120) {
            game.tick_recruits(0.1);
            game.tick_pawns(0.1);
        }
        let soldiers: Vec<_> = game
            .units
            .iter()
            .filter(|u| u.alive && u.faction == Faction::Blue && !u.is_player)
            .collect();
        assert!(
            !soldiers.is_empty(),
            "captured village must train soldiers in the owner's color"
        );
        for u in &soldiers {
            assert!(
                u.order.is_none(),
                "trained soldiers are normal field units, not stuck garrisons"
            );
        }
    }

    #[test]
    fn garrison_converts_on_capture() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        let (zx, zy) = (
            game.zone_manager.zones[3].center_gx,
            game.zone_manager.zones[3].center_gy,
        );
        let id = game.spawn_unit(UnitKind::Warrior, Faction::Villager, zx, zy, false);
        if let Some(u) = game.units.iter_mut().find(|u| u.id == id) {
            u.order = Some(crate::unit::OrderKind::DefendZone { zone: 3 });
        }
        game.convert_garrison(3, Faction::Red);
        let u = game.units.iter().find(|u| u.id == id).unwrap();
        assert_eq!(u.faction, Faction::Red, "survivors serve the new lord");
        assert_eq!(
            u.order,
            Some(crate::unit::OrderKind::DefendZone { zone: 3 }),
            "converted garrison keeps defending its village"
        );
    }

    #[test]
    fn militia_fights_intruders_and_holds_home() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        let z = &game.zone_manager.zones[3];
        let (zx, zy) = (z.center_gx, z.center_gy);
        let mid = game.spawn_unit(UnitKind::Warrior, Faction::Villager, zx + 2, zy, false);
        if let Some(u) = game.units.iter_mut().find(|u| u.id == mid) {
            u.order = Some(crate::unit::OrderKind::DefendZone { zone: 3 });
        }
        let rid = game.spawn_unit(UnitKind::Warrior, Faction::Red, zx + 4, zy, false);
        let input = crate::player_input::PlayerInput::default();
        let mut red_hurt = false;
        for _ in 0..(60 * 30) {
            game.tick(&input, 1.0 / 60.0);
            game.update(1.0 / 60.0);
            if let Some(r) = game.units.iter().find(|u| u.id == rid) {
                if r.hp < r.stats.max_hp {
                    red_hurt = true;
                    break;
                }
            } else {
                red_hurt = true; // dead counts
                break;
            }
        }
        assert!(red_hurt, "militia must engage intruders in its zone");
    }

    #[test]
    fn async_setup_matches_sync_setup() {
        let mut a = Game::new(960.0, 640.0);
        a.setup_demo_battle_with_seed(4242);

        let mut b = Game::new(960.0, 640.0);
        b.begin_async_setup(4242);
        let mut last = 0.0f32;
        let mut steps = 0u32;
        while b.setup_step() {
            let p = b.setup_progress();
            assert!(p >= last, "progress must be monotonic ({p} < {last})");
            last = p;
            steps += 1;
        }
        assert!(steps > 5, "budgeted setup should take several steps");
        assert!((b.setup_progress() - 1.0).abs() < f32::EPSILON);

        assert_eq!(a.grid.width, b.grid.width);
        for y in 0..a.grid.height {
            for x in 0..a.grid.width {
                assert_eq!(a.grid.get(x, y), b.grid.get(x, y), "tile ({x},{y})");
            }
        }
        assert_eq!(a.zone_manager.zones.len(), b.zone_manager.zones.len());
        assert_eq!(a.units.len(), b.units.len());
        assert_eq!(a.buildings.len(), b.buildings.len());
        assert_eq!(a.pawns.len(), b.pawns.len());
    }

    #[test]
    fn militia_sleeps_at_quiet_posts_and_wakes_on_hostiles() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        let z = &game.zone_manager.zones[3];
        let (zx, zy) = (z.center_gx, z.center_gy);
        let (zwx, zwy) = (z.center_wx, z.center_wy);
        let mid = game.spawn_unit(UnitKind::Warrior, Faction::Villager, zx + 2, zy, false);
        if let Some(u) = game.units.iter_mut().find(|u| u.id == mid) {
            u.order = Some(crate::unit::OrderKind::DefendZone { zone: 3 });
        }
        // A hostile far outside the settlement influence radius keeps the
        // battle running without waking the garrison.
        let rid = game.spawn_unit(UnitKind::Warrior, Faction::Red, zx.saturating_sub(60), zy, false);
        let input = crate::player_input::PlayerInput::default();
        let start = {
            let m = game.units.iter().find(|u| u.id == mid).unwrap();
            (m.x, m.y)
        };
        for _ in 0..60 {
            game.tick(&input, 1.0 / 60.0);
        }
        let m = game.units.iter().find(|u| u.id == mid).unwrap();
        assert_eq!(
            (m.x, m.y),
            start,
            "sleeping militia must not move without hostiles nearby"
        );
        // The hostile steps into the zone: the garrison wakes and engages.
        if let Some(r) = game.units.iter_mut().find(|u| u.id == rid) {
            r.x = zwx + 3.0 * crate::grid::TILE_SIZE;
            r.y = zwy;
        }
        let mut reacted = false;
        for _ in 0..(60 * 10) {
            game.tick(&input, 1.0 / 60.0);
            game.update(1.0 / 60.0);
            let Some(m) = game.units.iter().find(|u| u.id == mid) else {
                reacted = true; // died fighting — awake either way
                break;
            };
            if (m.x - start.0).abs() > 1.0
                || (m.y - start.1).abs() > 1.0
                || m.hp < m.stats.max_hp
            {
                reacted = true;
                break;
            }
        }
        assert!(reacted, "militia must wake when a hostile enters the zone");
    }

    #[test]
    fn capitals_have_buildings_at_every_map_size() {
        // Regression: the base placer clamped to the PLAYABLE_SIZE const,
        // leaving capitals beyond tile 176 bare on large maps; and extra
        // capitals could land side by side, interleaving their bands.
        for &(size, enemies) in &[(384u32, 3u8), (512, 2)] {
            let mut game = Game::new(960.0, 640.0);
            game.config.playable_size = size;
            game.config.enemy_count = enemies;
            game.setup_demo_battle_with_seed(777);
            let n_caps = 1 + enemies as usize;
            for zi in 0..n_caps {
                let z = &game.zone_manager.zones[zi];
                let prod = game
                    .buildings
                    .iter()
                    .filter(|b| {
                        b.produces.is_some()
                            && b.zone_id == Some(zi as u8)
                    })
                    .count();
                assert!(
                    prod >= 3,
                    "size {size} 1v{enemies}: capital {zi} has {prod} production buildings"
                );
                for other in game.zone_manager.zones[..n_caps].iter().skip(zi + 1) {
                    let dx = z.center_gx as f32 - other.center_gx as f32;
                    let dy = z.center_gy as f32 - other.center_gy as f32;
                    let d = (dx * dx + dy * dy).sqrt();
                    assert!(
                        d >= size as f32 * 0.3,
                        "size {size} 1v{enemies}: capitals only {d:.0} tiles apart"
                    );
                }
            }
        }
    }

    #[test]
    fn unaligned_villager_enlists_at_a_production_building() {
        let mut game = Game::new(960.0, 640.0);
        game.player_faction = None;
        game.untimed = true;
        game.setup_demo_battle_with_seed(42);

        let p = game.player_unit().expect("player spawns");
        assert_eq!(p.faction, Faction::Villager);
        assert_eq!(p.stats.max_hp, 5, "villager is fragile");
        let start_zone = game
            .zone_manager
            .zones
            .iter()
            .position(|z| {
                let d = (z.center_wx - p.x).powi(2) + (z.center_wy - p.y).powi(2);
                d < (z.radius as f32 * crate::grid::TILE_SIZE).powi(2)
            })
            .expect("pawn starts inside a settlement");
        assert!(
            game.zone_manager.zones[start_zone].owner.is_none(),
            "pawn starts at a neutral settlement"
        );

        // Soldiers never target the bystander.
        let (px, py) = (p.x, p.y);
        let rid = game.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            (px / crate::grid::TILE_SIZE) as u32 + 2,
            (py / crate::grid::TILE_SIZE) as u32,
            false,
        );
        let input = crate::player_input::PlayerInput::default();
        for _ in 0..120 {
            game.tick(&input, 1.0 / 60.0);
        }
        let pid = game.player_unit().unwrap().id;
        let r = game.units.iter().find(|u| u.id == rid).unwrap();
        assert_ne!(
            r.combat_target,
            Some(pid),
            "armies must not target the unaligned villager"
        );
        assert!(!game.player_attack(), "a bystander cannot attack");

        // Walk into a Blue production building: instant enlistment.
        let (bx, by, produced) = game
            .buildings
            .iter()
            .find(|b| {
                b.produces.is_some()
                    && b.zone_id.is_some_and(|z| {
                        game.zone_manager.zones[z as usize].effective_faction()
                            == Some(Faction::Blue)
                    })
            })
            .map(|b| (b.grid_x, b.grid_y, b.produces.unwrap()))
            .expect("blue capital has production");
        if let Some(u) = game.units.iter_mut().find(|u| u.is_player) {
            let (wx, wy) = crate::grid::grid_to_world(bx, by);
            u.x = wx;
            u.y = wy;
        }
        game.tick(&input, 1.0 / 60.0);
        assert_eq!(game.player_faction, Some(Faction::Blue));
        let p = game.player_unit().unwrap();
        assert_eq!(p.faction, Faction::Blue);
        assert_eq!(p.kind, produced);
        assert_eq!(p.hp, p.stats.max_hp, "enlistment heals to full");
    }

    #[test]
    fn garrison_joins_retinue_and_village_refills() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.authority = 100.0; // guaranteed acceptance rolls
        let zi = 3usize;
        let (zx, zy) = {
            let z = &game.zone_manager.zones[zi];
            (z.center_gx, z.center_gy)
        };
        game.zone_manager.zones[zi].set_controlled(Faction::Blue);
        game.resources[0] = [30, 30, 30];
        let gid = game.spawn_unit(UnitKind::Warrior, Faction::Blue, zx + 2, zy, false);
        let mid = game.spawn_unit(UnitKind::Warrior, Faction::Villager, zx - 2, zy, false);
        for &(id, zone) in &[(gid, zi as u8), (mid, zi as u8)] {
            if let Some(u) = game.units.iter_mut().find(|u| u.id == id) {
                u.order = Some(crate::unit::OrderKind::DefendZone { zone });
            }
        }
        // Park the player inside the zone.
        if let Some(p) = game.units.iter_mut().find(|u| u.is_player) {
            p.x = zx as f32 * crate::grid::TILE_SIZE;
            p.y = zy as f32 * crate::grid::TILE_SIZE;
        }
        let input = crate::player_input::PlayerInput::default();
        let mut joined = false;
        for _ in 0..(60 * 20) {
            game.tick(&input, 1.0 / 60.0);
            let g = game.units.iter().find(|u| u.id == gid).unwrap();
            if g.order == Some(crate::unit::OrderKind::Follow) {
                joined = true;
                break;
            }
        }
        assert!(joined, "own-color garrison must join the passing player");
        let m = game.units.iter().find(|u| u.id == mid).unwrap();
        assert!(
            matches!(m.order, Some(crate::unit::OrderKind::DefendZone { .. })),
            "neutral militia must never join the retinue"
        );
        // The owned village keeps spending stock to train fresh soldiers.
        let before: Vec<u32> = game
            .units
            .iter()
            .filter(|u| u.alive && u.faction == Faction::Blue)
            .map(|u| u.id)
            .collect();
        let mut trained = false;
        for _ in 0..(60 * 30) {
            game.tick(&input, 1.0 / 60.0);
            let fresh = game.units.iter().any(|u| {
                u.alive && u.faction == Faction::Blue && !u.is_player && !before.contains(&u.id)
            });
            if fresh {
                trained = true;
                break;
            }
        }
        assert!(trained, "the village must train replacements from stock");
    }

    #[test]
    fn garrison_monk_heals_wounded_in_quiet_zone() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        let zi = 3usize;
        let (zx, zy, zwx, zwy) = {
            let z = &game.zone_manager.zones[zi];
            (z.center_gx, z.center_gy, z.center_wx, z.center_wy)
        };
        game.zone_manager.zones[zi].set_controlled(Faction::Blue);
        let monk = game.spawn_unit(UnitKind::Monk, Faction::Blue, zx + 2, zy, false);
        if let Some(u) = game.units.iter_mut().find(|u| u.id == monk) {
            u.order = Some(crate::unit::OrderKind::DefendZone { zone: zi as u8 });
        }
        // A wounded ally rests at the zone edge, no hostiles anywhere near.
        let hurt = game.spawn_unit(UnitKind::Warrior, Faction::Blue, zx - 3, zy, false);
        if let Some(u) = game.units.iter_mut().find(|u| u.id == hurt) {
            u.hp = 3;
            u.x = zwx - 3.0 * crate::grid::TILE_SIZE;
            u.y = zwy;
        }
        // Distant Red keeps the battle alive without waking the zone.
        game.spawn_unit(UnitKind::Warrior, Faction::Red, zx.saturating_sub(60), zy, false);
        let input = crate::player_input::PlayerInput::default();
        let mut healed = false;
        for _ in 0..(60 * 30) {
            game.tick(&input, 1.0 / 60.0);
            let h = game.units.iter().find(|u| u.id == hurt).unwrap();
            if h.hp > 3 {
                healed = true;
                break;
            }
        }
        assert!(healed, "stationed monk must patrol to and heal the wounded ally");
    }

    #[test]
    fn hold_zone_order_stations_retinue() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        let (px, py) = {
            let p = game.player_unit().unwrap();
            (p.x, p.y)
        };
        // Give the player a follower next to them
        let fid = game.spawn_unit(
            UnitKind::Warrior,
            Faction::Blue,
            (px / crate::grid::TILE_SIZE) as u32 + 1,
            (py / crate::grid::TILE_SIZE) as u32,
            false,
        );
        if let Some(u) = game.units.iter_mut().find(|u| u.id == fid) {
            u.order = Some(crate::unit::OrderKind::Follow);
        }
        let outcome = game.issue_order(crate::unit::OrderRequest::HoldZone);
        assert!(matches!(outcome, crate::unit::OrderOutcome::Issued(1)));
        let u = game.units.iter().find(|u| u.id == fid).unwrap();
        assert!(
            matches!(u.order, Some(crate::unit::OrderKind::DefendZone { .. })),
            "stationed unit must carry the DefendZone stance"
        );
        assert!(
            u.re_recruit_cooldown > 0.0,
            "stationed units leave the retinue with a re-recruit cooldown"
        );
    }

    #[test]
    fn levy_stalls_without_resources() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.retain(|u| u.is_player);
        game.pawns.clear(); // no workers: no income to restock the chests
        game.resources = [[0; 3]; 5];
        for _ in 0..(10 * 60) {
            game.tick_recruits(0.1);
            game.tick_pawns(0.1);
        }
        assert!(
            !game
                .units
                .iter()
                .any(|u| u.alive && u.faction == Faction::Villager),
            "no resources, no levy"
        );
    }


    /// Game with a 3-zone layout and a bleed threshold of 2 (majority of 3).
    fn game_with_three_zones() -> Game {
        let mut game = Game::new(960.0, 640.0);
        let layout = crate::mapgen::MapLayout {
            blue_base: (20, 20),
            red_base: (139, 139),
            zone_centers: vec![(50, 50), (80, 80), (110, 110)],
            blue_gather: (21, 21),
            red_gather: (138, 138),
            blue_home_zones: vec![0],
            red_home_zones: vec![2],
            connections: vec![vec![1], vec![0, 2], vec![1]],
            settlements: Vec::new(),
            extra_bases: Vec::new(),
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout, game.config.zone_radius);
        game
    }

    #[test]
    fn typed_costs_shape_the_army() {
        let run = |pools: [u32; 3]| -> Vec<UnitKind> {
            let mut game = Game::new(960.0, 640.0);
            game.setup_demo_battle_with_seed(42);
            game.units.retain(|u| u.is_player);
            game.pawns.clear(); // fixed chests: no worker income
            game.resources = [[0; 3]; 5];
            game.resources[0] = pools;
            for _ in 0..(10 * 150) {
                game.tick_recruits(0.1);
                game.tick_pawns(0.1);
            }
            game.units
                .iter()
                .filter(|u| u.alive && u.faction == Faction::Blue && !u.is_player)
                .map(|u| u.kind)
                .collect()
        };

        // Meat + gold only: steel and scripture, no bows or lances.
        let gold_army = run([30, 30, 0]);
        assert!(!gold_army.is_empty(), "gold economy must field soldiers");
        assert!(
            gold_army
                .iter()
                .all(|&k| k == UnitKind::Warrior || k == UnitKind::Monk),
            "without wood only gold units can be armed: {gold_army:?}"
        );

        // Meat + wood only: bows and lances.
        let wood_army = run([30, 0, 30]);
        assert!(!wood_army.is_empty(), "wood economy must field soldiers");
        assert!(
            wood_army
                .iter()
                .all(|&k| k == UnitKind::Archer || k == UnitKind::Lancer),
            "without gold only wood units can be armed: {wood_army:?}"
        );
    }

    #[test]
    fn landless_army_starves_and_loses() {
        let mut game = game_with_three_zones();
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        for z in &mut game.zone_manager.zones {
            z.set_controlled(Faction::Blue);
        }
        // A landless Red remnant with no home left.
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        for _ in 0..120 {
            game.tick_zones(1.0);
            if game.winner.is_some() {
                break;
            }
        }
        assert_eq!(
            game.winner,
            Some(Faction::Blue),
            "starvation must finish a decided war"
        );
    }

    #[test]
    fn conquest_eliminates_landless_armyless_faction() {
        let mut game = game_with_three_zones();
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Red owns nothing and fields nobody → eliminated, Blue wins.
        game.tick_zones(0.1);
        assert_eq!(game.winner, Some(Faction::Blue));
    }

    #[test]
    fn faction_stands_with_units_or_settlements() {
        let mut game = game_with_three_zones();
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        game.tick_zones(0.1);
        assert_eq!(game.winner, None, "both armies alive");

        // Red army destroyed but it still owns a settlement: it can
        // retrain, so the war is not over.
        game.zone_manager.zones[2].set_controlled(Faction::Red);
        for u in &mut game.units {
            if u.faction == Faction::Red {
                u.alive = false;
            }
        }
        game.tick_zones(0.1);
        assert_eq!(game.winner, None, "a landed faction can rebuild");

        // A landless remnant army also keeps a faction standing.
        game.zone_manager.zones[2].set_controlled(Faction::Blue);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        game.tick_zones(0.1);
        assert_eq!(game.winner, None, "a landless army fights on");
    }
}

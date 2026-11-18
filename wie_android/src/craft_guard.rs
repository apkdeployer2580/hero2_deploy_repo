// ★★ 2026-10-20 — 아이템 조합 × 가득 찬 가방 (user: *"아이템창이 풀일때 아이템 조합시 성공판정이 되면 소멸되버리는데
// 아이템창 풀이면 조합을 시도하려고하면 가방이 가득찼다고 안내가 나오게 해줘"*).
//
// The crafting window `aj` (§8.60) crafts on OK with its cursor on row 0 (`aj.a(II)Z` @0x1167ac): materials present
// (`aj.a()Z` — every `aj.i[k]` for `k < aj.a:B` is not −1), gold ≥ `aj.a:I`, then `aj.b ← 3` and the next paint runs
// `aj.d()V` @0x116d90 — gold, the materials (`bag.a(BBB)V(type, id, 1)` each), the recipe book (`ag.b.a(23, id, 1)`) all
// consumed, THEN the result `z.a(BBZZ)` is handed to `bq.a(Lch;B)I` of the 재료가방 (a material type) or the inventory.
// That insert has no room check of its own: a full bag places nothing and the result is gone. The host reads the same
// decision before the game does and, when the result's bag will have no room, swallows that OK and raises the game's own
// 「빈공간이 없습니다.」 box (`ba`, itmtp 7) on `aj` instead — the materials, the book and the gold stay. The game's own
// refusals (materials short, gold short) are left to it: the guard only speaks when the game WOULD craft.
//
// ★ "Will have" — the consumption comes first: a material whose record holds exactly one (`ch.c ≤ 1`) leaves its bag and
// FREES a cell before the result is placed (probe: a 28/28 inventory with the recipe's blade in it took the result — the
// blade's cell). So the free cells counted are `capacity − count` plus the records the craft removes from that bag (the
// book, in the 재료가방, counts too); a stack of the same `(type, id)` under 99 still merges.
//
// Pure readers — the app (`emulator::craft_full_guard`) and the probe (`--h2-craft-guard`) share them.

impl Hero2Java {
    /// The crafting window `aj` (desc 0x1923a0, parent `j` = 0x28 bytes): its declared fields by absolute offset.
    pub const CRAFT_CLASS_DESC: u32 = 0x0019_23a0;
    /// `aj.a:Lae;` — the recipe book chosen from the picker.
    pub const CRAFT_RECIPE_OFF: u32 = 0x28 + 0x4;
    /// `aj.a:I` — the cost (1,000, `aj.c()V`).
    pub const CRAFT_COST_OFF: u32 = 0x28 + 0xc;
    /// `aj.a:B` — how many materials the book names.
    pub const CRAFT_MATERIALS_OFF: u32 = 0x28 + 0x10;
    /// `aj.i:[B` — a bag index per material, −1 = not found (`aj.c()V`; the +50 mark is patched off, `craft_material_mark`).
    pub const CRAFT_INDICES_OFF: u32 = 0x28 + 0x14;
    /// `aj.b:B` — the stage: 3 crafting · 4 result shown · 5 closing (`aj.a(II)Z`); anything else = the list.
    pub const CRAFT_STAGE_OFF: u32 = 0x28 + 0x18;
    /// The recipe record `ae` (parent `ch` = 0x20 bytes): `ae.a:B` result type · `ae.b:B` result id · `ae.i:[B` material
    /// types · `ae.l:[B` material ids (one each, in `aj.i`'s order).
    pub const RECIPE_RESULT_TYPE_OFF: u32 = 0x20;
    pub const RECIPE_RESULT_ID_OFF: u32 = 0x20 + 0x4;
    pub const RECIPE_MAT_TYPES_OFF: u32 = 0x20 + 0x8;
    pub const RECIPE_MAT_IDS_OFF: u32 = 0x20 + 0xc;
}

/// The alchemist's window chain (state 8, rooted at `aw.a` — `aw` → `aj` → the picker `k` / the box `ba`), root first.
/// `None` off state 8, or when the root is not the alchemist's menu. The picker walker's chain
/// ([`h2_picker_chain`]) without the field-menu branch and the origin.
pub fn h2_alchemist_chain(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &Hero2Java) -> Option<Vec<H2Win>> {
    let _ = h2;
    if !h2_npc_window_up(read) {
        return None;
    }
    let mut out = Vec::with_capacity(4);
    let mut handle = h2_static_handle(read, Hero2Java::ALCHEMIST_SLOT)?;
    for _ in 0..6 {
        let fields = h2_fields(read, handle)?;
        let class = h2_class_desc(read, handle)?;
        if out.is_empty() && class != Hero2Java::ALCHEMIST_CLASS_DESC {
            return None;
        }
        out.push(H2Win { handle, fields, class });
        match h2_rd_u32(read, fields + 4 + Hero2Java::WIN_CHILD_OFF).filter(|p| h2_in_java_heap(*p)) {
            Some(child) => handle = child,
            None => break,
        }
    }
    Some(out)
}

/// Why the crafting OK is being refused — for the log and the probe's gauge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct H2CraftOverflow {
    /// The crafting window `aj` the box goes on.
    pub window: u32,
    /// The result the recipe would make.
    pub result: (i8, i8),
    /// The bag it would go to (`ag.b` for a material type, `ag.a` otherwise), its records / capacity now, and the
    /// records the craft itself takes out of it before the insert (materials and the book that run out, split stacks
    /// the bag's compaction merges).
    pub bag: u32,
    pub count: u32,
    pub capacity: i8,
    pub freed: u32,
}

/// One bag's records as the craft will leave them: `(handle, type, id, count)`.
type CraftRecs = Vec<(u32, i8, i8, i32)>;

/// `bq.e()V` @0x13815c — the compaction `bq.a(BBB)V` runs on a bag after consuming from it: for every stackable record
/// (`bq.d(B)Z`) under 99, every LATER record of the same `(type, id)` is poured into it — whole (and removed) when the
/// two fit in 99, else up to 99 with the remainder left. Greedy, in Vector order; equipment never merges.
fn craft_compact(recs: &mut CraftRecs) {
    let mut i = 0;
    while i < recs.len() {
        let (kind, id) = (recs[i].1, recs[i].2);
        if h2_stackable_type(kind) && recs[i].3 <= Hero2Java::STACK_MAX - 1 {
            let mut j = i + 1;
            while j < recs.len() {
                if recs[j].1 == kind && recs[j].2 == id {
                    let room = Hero2Java::STACK_MAX - recs[i].3;
                    if recs[j].3 <= room {
                        recs[i].3 += recs[j].3;
                        recs.remove(j);
                        continue;
                    }
                    recs[j].3 -= room;
                    recs[i].3 = Hero2Java::STACK_MAX;
                }
                j += 1;
            }
        }
        i += 1;
    }
}

/// Would this OK craft a result the bag cannot hold? `Some` exactly when the game's own `aj.a(II)Z` would reach
/// `aj.b ← 3` — the crafting window on top of the alchemist's chain, in the list stage, cursor on row 0 (「조합」),
/// every material found, gold enough — AND the result's bag will be full once the craft has taken its materials and
/// the book (`bq.a(BI)V` removes a record that runs out) and compacted the bag it took from (`bq.e()V`, split stacks
/// merge), with no stack of the same `(type, id)` under 99 for the game's own merge to fill. `None` whenever the game
/// would refuse or close by itself, or there is room. A bag whose records cannot all be read is judged by its header
/// alone (removed records counted, no compaction) — never by a guess that frees more.
pub fn h2_craft_overflow(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &Hero2Java) -> Option<H2CraftOverflow> {
    let chain = h2_alchemist_chain(read, h2)?;
    let top = chain.last()?;
    if top.class != Hero2Java::CRAFT_CLASS_DESC {
        return None;
    }
    let f = top.fields + 4;
    let stage = h2_rd_i8(read, f + Hero2Java::CRAFT_STAGE_OFF)?;
    if (3..=5).contains(&stage) {
        return None;
    }
    if h2_rd_i8(read, f + Hero2Java::WIN_CURSOR_OFF)? != 0 {
        return None;
    }
    // `aj.a()Z`: every named material was found.
    let n = h2_rd_i8(read, f + Hero2Java::CRAFT_MATERIALS_OFF)?;
    if n <= 0 {
        return None;
    }
    let n = n as usize;
    let indices = h2_byte_array(read, h2_rd_u32(read, f + Hero2Java::CRAFT_INDICES_OFF).filter(|p| h2_in_java_heap(*p))?)?;
    if indices.len() < n || indices[..n].iter().any(|&i| i == 0xff) {
        return None;
    }
    // Gold enough (`ag.f:I`).
    let cost = h2_rd_u32(read, f + Hero2Java::CRAFT_COST_OFF)? as i32;
    let gold = h2_rd_u32(read, ShopSellPatch::GOLD_SLOT)? as i32;
    if cost > gold {
        return None;
    }
    // The recipe: its result, its materials, and the book record itself (the `ae` IS the record in the 재료가방).
    let recipe = h2_rd_u32(read, f + Hero2Java::CRAFT_RECIPE_OFF).filter(|p| h2_in_java_heap(*p))?;
    let rf = h2_fields(read, recipe)? + 4;
    let result = (h2_rd_i8(read, rf + Hero2Java::RECIPE_RESULT_TYPE_OFF)?, h2_rd_i8(read, rf + Hero2Java::RECIPE_RESULT_ID_OFF)?);
    let mat_types = h2_byte_array(read, h2_rd_u32(read, rf + Hero2Java::RECIPE_MAT_TYPES_OFF).filter(|p| h2_in_java_heap(*p))?)?;
    if mat_types.len() < n {
        return None;
    }
    let heads = [h2_bag_head(read, h2.inventory_slot)?, h2_bag_head(read, h2.material_bag_slot)?];
    let dest = usize::from(h2_material_type(result.0));
    // What the craft takes: `(bag, record handle)` per material, then the book from the 재료가방.
    let mut taken: Vec<(usize, u32)> = Vec::with_capacity(n + 1);
    for k in 0..n {
        let bag = usize::from(h2_material_type(mat_types[k] as i8));
        taken.push((bag, h2_bag_element(read, heads[bag].handle, u32::from(indices[k]))?));
    }
    taken.push((1, recipe));
    let compacts = [taken.iter().any(|&(b, _)| b == 0), true];
    // The destination bag as the craft leaves it.
    let head = &heads[dest];
    let full: Option<CraftRecs> = h2_bag_of(read, head.handle).map(|b| b.items.iter().map(|it| (it.handle, it.kind, it.id, it.count)).collect());
    let left: u32;
    let mut recs = full.filter(|r| r.len() as u32 == head.count);
    match recs.as_mut() {
        Some(recs) => {
            for &(_, handle) in taken.iter().filter(|&&(b, _)| b == dest) {
                if let Some(r) = recs.iter_mut().find(|r| r.0 == handle) {
                    r.3 -= 1;
                }
            }
            recs.retain(|r| r.3 > 0);
            if compacts[dest] {
                craft_compact(recs);
            }
            left = recs.len() as u32;
        }
        None => {
            // Unreadable records: only what is certainly removed counts — a record of one that the craft takes.
            let mut removed = 0u32;
            for &(_, handle) in taken.iter().filter(|&&(b, _)| b == dest) {
                if h2_item_at(read, handle).is_some_and(|r| r.count <= 1) {
                    removed += 1;
                }
            }
            left = head.count.saturating_sub(removed);
        }
    }
    if head.b > 0 && left < head.b as u32 {
        return None;
    }
    if h2_stackable_type(result.0) {
        let merges = match recs.as_ref() {
            Some(recs) => recs.iter().any(|r| r.1 == result.0 && r.2 == result.1 && r.3 < Hero2Java::STACK_MAX),
            None => h2_bag_of(read, head.handle).is_some_and(|b| b.items.iter().any(|it| it.kind == result.0 && it.id == result.1 && it.count < Hero2Java::STACK_MAX)),
        };
        if merges {
            return None;
        }
    }
    Some(H2CraftOverflow { window: top.handle, result, bag: head.handle, count: head.count, capacity: head.b, freed: head.count.saturating_sub(left) })
}

#[cfg(test)]
mod craft_guard_tests {
    use super::*;

    struct Mem(std::collections::BTreeMap<u32, u8>);
    impl Mem {
        fn new() -> Self {
            Mem(std::collections::BTreeMap::new())
        }
        fn put(&mut self, a: u32, b: &[u8]) {
            for (i, x) in b.iter().enumerate() {
                self.0.insert(a + i as u32, *x);
            }
        }
        fn u32(&mut self, a: u32, v: u32) {
            self.put(a, &v.to_le_bytes());
        }
        fn reader(&self) -> impl Fn(u32, usize) -> Vec<u8> + '_ {
            move |a, n| (0..n as u32).map_while(|i| self.0.get(&(a + i)).copied()).collect()
        }
        fn object(&mut self, handle: u32, fields: u32, class: u32, desc: u32) {
            self.u32(handle, fields);
            self.u32(handle + 4, class);
            self.u32(class + 8, desc);
        }
        fn bytes(&mut self, handle: u32, fields: u32, data: &[u8]) {
            self.u32(handle, fields);
            self.u32(fields + 4, data.len() as u32);
            self.put(fields + 8, data);
        }
    }

    fn h2() -> Hero2Java {
        GuestBuild::Hero2Ktf.hero2_java().unwrap()
    }

    fn state_slot() -> u32 {
        match GuestBuild::Hero2Ktf.app_state() {
            Some(AppState::Absolute { addr, .. }) => addr,
            other => panic!("hero2 has an absolute state word, got {other:?}"),
        }
    }

    /// A bag at static `slot`: capacity `b`, `records` of `(type, id, count)` in index order.
    fn bag(m: &mut Mem, slot: u32, base: u32, b: i8, records: &[(i8, i8, i32)]) -> u32 {
        let (bag, bf) = (base, base + 0x1000);
        m.u32(slot, bag);
        m.object(bag, bf, base + 0x2000, 0x19_2844);
        m.put(bf + 4 + Hero2Java::INV_B_OFF, &[b as u8]);
        m.put(bf + 4 + Hero2Java::INV_HEAL_OFF, &[0]);
        m.put(bf + 4 + Hero2Java::INV_C_OFF, &[0]);
        let (vec, vf) = (base + 0x3000, base + 0x4000);
        m.u32(bf + 4 + Hero2Java::INV_VECTOR_OFF, vec);
        m.object(vec, vf, base + 0x5000, 0);
        let (data, df) = (base + 0x6000, base + 0x7000);
        m.u32(vf + 4 + Hero2Java::VECTOR_DATA_OFF, data);
        m.u32(vf + 4 + Hero2Java::VECTOR_COUNT_OFF, records.len() as u32);
        m.object(data, df, base + 0x8000, 0);
        m.u32(df + 4, 256);
        let mut book_placed = false;
        for (k, (kind, id, count)) in records.iter().enumerate() {
            // The recipe book IS the `ae` the crafting window holds (`aj.a:Lae;` = `bag.a(sel)`): the first type-23
            // record is that object, by handle — the craft takes the book from the bag through it.
            let (rh, rf) = if *kind == 23 && !book_placed {
                book_placed = true;
                (AE, AE_F)
            } else {
                (base + 0x10000 + k as u32 * 0x100, base + 0x30000 + k as u32 * 0x100)
            };
            m.object(rh, rf, if rh == AE { AE_C } else { base + 0x9000 }, if rh == AE { 0x19_22ec } else { 0x19_2aa8 });
            m.put(rf + 4 + Hero2Java::ITEM_TYPE_OFF, &[*kind as u8]);
            m.put(rf + 4 + Hero2Java::ITEM_ID_OFF, &[*id as u8]);
            m.u32(rf + 4 + Hero2Java::ITEM_COUNT_OFF, *count as u32);
            m.u32(df + 8 + k as u32 * 4, rh);
        }
        bag
    }

    const AJ: u32 = 0x4811_0000;
    const AJ_F: u32 = 0x4811_0100;
    /// The recipe record `ae` — the crafting window's book and, in a test's material bag, the first type-23 record.
    const AE: u32 = 0x4813_0000;
    const AE_F: u32 = 0x4813_0100;
    const AE_C: u32 = 0x4813_0200;
    const INV: u32 = 0x4a00_0000;
    const MATS: u32 = 0x4b00_0000;

    /// The alchemist's chain `aw` → `aj` with the craft set up: cost 1,000, the recipe's result and its materials
    /// `(type, id, bag index)` (index −1 = not found), the book's own count, stage, cursor, gold.
    #[allow(clippy::too_many_arguments)]
    fn craft(m: &mut Mem, stage: i8, cursor: i8, gold: i32, mats: &[(i8, i8, i8)], book_count: i32, result: (i8, i8)) {
        m.u32(state_slot(), Hero2Java::NPC_WINDOW_STATE);
        let (aw, aw_f, aw_c) = (0x4810_0000u32, 0x4810_0100u32, 0x4810_0200u32);
        m.u32(Hero2Java::ALCHEMIST_SLOT, aw);
        m.object(aw, aw_f, aw_c, Hero2Java::ALCHEMIST_CLASS_DESC);
        m.object(AJ, AJ_F, 0x4811_0200, Hero2Java::CRAFT_CLASS_DESC);
        m.u32(aw_f + 4 + Hero2Java::WIN_CHILD_OFF, AJ);
        m.u32(AJ_F + 4 + Hero2Java::WIN_CHILD_OFF, 0);
        m.put(AJ_F + 4 + Hero2Java::WIN_CURSOR_OFF, &[cursor as u8]);
        m.put(AJ_F + 4 + Hero2Java::CRAFT_STAGE_OFF, &[stage as u8]);
        m.put(AJ_F + 4 + Hero2Java::CRAFT_MATERIALS_OFF, &[mats.len() as u8]);
        m.u32(AJ_F + 4 + Hero2Java::CRAFT_COST_OFF, 1000);
        let (idx, idx_f) = (0x4812_0000u32, 0x4812_0100u32);
        m.bytes(idx, idx_f, &mats.iter().map(|&(_, _, i)| i as u8).collect::<Vec<u8>>());
        m.u32(AJ_F + 4 + Hero2Java::CRAFT_INDICES_OFF, idx);
        let (ae, ae_f) = (AE, AE_F);
        m.object(ae, ae_f, AE_C, 0x19_22ec);
        m.put(ae_f + 4 + Hero2Java::ITEM_TYPE_OFF, &[23]);
        m.put(ae_f + 4 + Hero2Java::ITEM_ID_OFF, &[0]);
        m.u32(ae_f + 4 + Hero2Java::ITEM_COUNT_OFF, book_count as u32);
        m.put(ae_f + 4 + Hero2Java::RECIPE_RESULT_TYPE_OFF, &[result.0 as u8]);
        m.put(ae_f + 4 + Hero2Java::RECIPE_RESULT_ID_OFF, &[result.1 as u8]);
        let (ty, ty_f, id, id_f) = (0x4814_0000u32, 0x4814_0100u32, 0x4815_0000u32, 0x4815_0100u32);
        m.bytes(ty, ty_f, &mats.iter().map(|&(t, _, _)| t as u8).collect::<Vec<u8>>());
        m.bytes(id, id_f, &mats.iter().map(|&(_, i, _)| i as u8).collect::<Vec<u8>>());
        m.u32(ae_f + 4 + Hero2Java::RECIPE_MAT_TYPES_OFF, ty);
        m.u32(ae_f + 4 + Hero2Java::RECIPE_MAT_IDS_OFF, id);
        m.u32(AJ_F + 4 + Hero2Java::CRAFT_RECIPE_OFF, ae);
        m.u32(ShopSellPatch::GOLD_SLOT, gold as u32);
    }

    /// Recipe 0 「헤비 블레이드」 as shipped: (0,2) 블레이드 from the inventory, (30,1) 강철 and (32,1) 연마가루 from the
    /// 재료가방 — the material bag holds the book at 0, 강철 ×5 at 1, 연마가루 ×10 at 2.
    const MATS_SHIPPED: [(i8, i8, i8); 3] = [(0, 2, 2), (30, 1, 1), (32, 1, 2)];

    fn material_bag(m: &mut Mem, b: i8) -> u32 {
        bag(m, h2().material_bag_slot, MATS, b, &[(23, 0, 1), (30, 1, 5), (32, 1, 10)])
    }

    /// Three records in a 3-cell inventory, the blade (×1) at index 2: full — but the blade frees its cell.
    fn full_inventory_with_blade(m: &mut Mem, blade_count: i32) -> u32 {
        bag(m, h2().inventory_slot, INV, 3, &[(5, 1, 1), (6, 2, 1), (0, 2, blade_count)])
    }

    #[test]
    fn a_material_running_out_in_the_full_bag_frees_the_cell_the_result_takes() {
        // The probe's own case: inventory 28/28 with the blade ×1 in it → the game placed the result (27 records after).
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &MATS_SHIPPED, 1, (0, 16));
        full_inventory_with_blade(&mut m, 1);
        material_bag(&mut m, 28);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2()), None, "the blade's cell frees first");
        // The blade stacked ×2 stays a record: nothing frees, the result would be lost.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &MATS_SHIPPED, 1, (0, 16));
        let bag = full_inventory_with_blade(&mut m, 2);
        material_bag(&mut m, 28);
        let got = h2_craft_overflow(&m.reader(), &h2()).expect("refused");
        assert_eq!(got, H2CraftOverflow { window: AJ, result: (0, 16), bag, count: 3, capacity: 3, freed: 0 });
    }

    #[test]
    fn a_craft_into_a_full_inventory_with_materials_elsewhere_is_refused() {
        // All three materials in the 재료가방 (a 10-cell bag), the inventory 3/3 of other things.
        let mats = [(30, 1, 1), (32, 1, 2), (31, 0, 3)];
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &mats, 1, (0, 16));
        let inv = bag(&mut m, h2().inventory_slot, INV, 3, &[(5, 1, 1), (6, 2, 1), (7, 3, 1)]);
        bag(&mut m, h2().material_bag_slot, MATS, 10, &[(23, 0, 1), (30, 1, 5), (32, 1, 10), (31, 0, 1)]);
        let got = h2_craft_overflow(&m.reader(), &h2()).expect("refused");
        assert_eq!(got, H2CraftOverflow { window: AJ, result: (0, 16), bag: inv, count: 3, capacity: 3, freed: 0 });
    }

    #[test]
    fn the_games_own_refusals_and_a_bag_with_room_are_left_to_the_game() {
        let h2 = h2();
        let full = |m: &mut Mem| {
            bag(m, h2.inventory_slot, INV, 3, &[(5, 1, 1), (6, 2, 1), (0, 2, 2)]);
            material_bag(m, 28);
        };
        // Room: 3 of 4.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &MATS_SHIPPED, 1, (0, 16));
        bag(&mut m, h2.inventory_slot, INV, 4, &[(5, 1, 1), (6, 2, 1), (0, 2, 2)]);
        material_bag(&mut m, 28);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "a cell is free");
        // A material missing (−1): the game says 「재료가 부족…」 itself.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(0, 2, 2), (30, 1, -1), (32, 1, 2)], 1, (0, 16));
        full(&mut m);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "the game refuses a missing material");
        // Gold short: the game says so itself.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 999, &MATS_SHIPPED, 1, (0, 16));
        full(&mut m);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "the game refuses short gold");
        // Cursor on 「취소」 (row 1): OK closes.
        let mut m = Mem::new();
        craft(&mut m, 0, 1, 5000, &MATS_SHIPPED, 1, (0, 16));
        full(&mut m);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "row 1 is cancel");
        // Stages 3 · 4 · 5: crafting, result, closing — OK is not a craft.
        for stage in 3..=5 {
            let mut m = Mem::new();
            craft(&mut m, stage, 0, 5000, &MATS_SHIPPED, 1, (0, 16));
            full(&mut m);
            assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "stage {stage}");
        }
        // Not the alchemist's window at all (state 5 field menu): nothing.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &MATS_SHIPPED, 1, (0, 16));
        full(&mut m);
        m.u32(state_slot(), 5);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "off state 8");
    }

    #[test]
    fn a_material_result_counts_the_material_bag_where_the_book_and_stacks_free_cells() {
        let h2 = h2();
        // The result is a material type (23 조합서) and the 재료가방 (3 cells) is full — but the book (×1) leaves: room.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(0, 2, 0)], 1, (23, 5));
        bag(&mut m, h2.inventory_slot, INV, 127, &[(0, 2, 1)]);
        bag(&mut m, h2.material_bag_slot, MATS, 3, &[(23, 0, 1), (30, 1, 5), (32, 1, 10)]);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "the book's cell frees");
        // The book stacked ×2 stays: full → refused, and it is the material bag that is named.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(0, 2, 0)], 2, (23, 5));
        bag(&mut m, h2.inventory_slot, INV, 127, &[(0, 2, 1)]);
        let mats = bag(&mut m, h2.material_bag_slot, MATS, 3, &[(23, 0, 2), (30, 1, 5), (32, 1, 10)]);
        let got = h2_craft_overflow(&m.reader(), &h2).expect("the material bag is full");
        assert_eq!((got.bag, got.result, got.freed), (mats, (23, 5), 0));
        // …unless a stack of the same (type, id) under 99 is there for the merge.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(0, 2, 0)], 2, (23, 5));
        bag(&mut m, h2.inventory_slot, INV, 127, &[(0, 2, 1)]);
        bag(&mut m, h2.material_bag_slot, MATS, 3, &[(23, 0, 2), (23, 5, 1), (32, 1, 10)]);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "the merge fills the stack");
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(0, 2, 0)], 2, (23, 5));
        bag(&mut m, h2.inventory_slot, INV, 127, &[(0, 2, 1)]);
        bag(&mut m, h2.material_bag_slot, MATS, 3, &[(23, 0, 2), (23, 5, 99), (32, 1, 10)]);
        assert!(h2_craft_overflow(&m.reader(), &h2).is_some(), "a stack of 99 takes no more");
        // Equipment never merges: the same (type, id) in the bag changes nothing.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(30, 1, 1)], 2, (5, 1));
        bag(&mut m, h2.inventory_slot, INV, 1, &[(5, 1, 1)]);
        bag(&mut m, h2.material_bag_slot, MATS, 3, &[(23, 0, 2), (30, 1, 5)]);
        assert!(h2_craft_overflow(&m.reader(), &h2).is_some(), "equipment does not stack");
    }

    #[test]
    fn the_bags_own_compaction_after_a_consume_frees_split_stacks() {
        let h2 = h2();
        // Recipe 53's shape: 제련석 (20,0) ×25 in the inventory, the other two in the 재료가방. The inventory is 3/3 —
        // but it holds a split potion stack (24,0) ×85 + ×1, and consuming the 제련석 there runs `bq.e()V` on it:
        // the two merge (×86), a cell frees, the result lands. (The probe saw exactly this: 28/28 → result placed.)
        let mats = [(30, 4, 1), (20, 0, 1), (31, 1, 2)];
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &mats, 1, (18, 1));
        bag(&mut m, h2.inventory_slot, INV, 3, &[(24, 0, 85), (20, 0, 25), (24, 0, 1)]);
        bag(&mut m, h2.material_bag_slot, MATS, 28, &[(23, 53, 1), (30, 4, 5), (31, 1, 7)]);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "the split potions merge in the compaction");
        // The same split, but no material taken from the inventory: it is not compacted → refused.
        let mats = [(30, 4, 1), (31, 1, 2)];
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &mats, 1, (18, 1));
        let inv = bag(&mut m, h2.inventory_slot, INV, 3, &[(24, 0, 85), (20, 0, 25), (24, 0, 1)]);
        bag(&mut m, h2.material_bag_slot, MATS, 28, &[(23, 53, 1), (30, 4, 5), (31, 1, 7)]);
        let got = h2_craft_overflow(&m.reader(), &h2).expect("nothing frees");
        assert_eq!((got.bag, got.freed), (inv, 0));
        // Two full stacks (×99 + ×99) do not merge; ×60 + ×60 merge to ×99 + ×21 and free nothing either.
        for (a, b) in [(99, 99), (60, 60)] {
            let mut m = Mem::new();
            craft(&mut m, 0, 0, 5000, &[(30, 4, 1), (20, 0, 1), (31, 1, 2)], 1, (18, 1));
            bag(&mut m, h2.inventory_slot, INV, 3, &[(24, 0, a), (20, 0, 25), (24, 0, b)]);
            bag(&mut m, h2.material_bag_slot, MATS, 28, &[(23, 53, 1), (30, 4, 5), (31, 1, 7)]);
            assert!(h2_craft_overflow(&m.reader(), &h2).is_some(), "{a}+{b} frees no cell");
        }
        // The 재료가방 is always compacted (the book is taken from it): the result (30,3) of recipe 58 lands when a
        // split 강철 stack merges there.
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(30, 1, 1), (20, 0, 0), (31, 0, 3)], 2, (30, 3));
        bag(&mut m, h2.inventory_slot, INV, 127, &[(20, 0, 25)]);
        bag(&mut m, h2.material_bag_slot, MATS, 4, &[(23, 58, 2), (30, 1, 5), (30, 1, 2), (31, 0, 2)]);
        assert_eq!(h2_craft_overflow(&m.reader(), &h2), None, "강철 ×5 + ×2 merge");
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &[(30, 1, 1), (20, 0, 0), (31, 0, 3)], 2, (30, 3));
        bag(&mut m, h2.inventory_slot, INV, 127, &[(20, 0, 25)]);
        let mats_bag = bag(&mut m, h2.material_bag_slot, MATS, 4, &[(23, 58, 2), (30, 1, 5), (32, 1, 2), (31, 0, 2)]);
        let got = h2_craft_overflow(&m.reader(), &h2).expect("book ×2 · 31:0 ×2 · nothing splits — full");
        assert_eq!((got.bag, got.result, got.freed), (mats_bag, (30, 3), 0));
    }

    #[test]
    fn the_notice_target_finds_the_crafting_window_in_the_alchemists_chain() {
        let mut m = Mem::new();
        craft(&mut m, 0, 0, 5000, &MATS_SHIPPED, 1, (0, 16));
        let h2 = h2();
        let (win, chain) = h2_notice_target(&m.reader(), &h2, AJ, None).expect("aj is the top of the alchemist's chain");
        assert_eq!((win.handle, win.class, chain.len()), (AJ, Hero2Java::CRAFT_CLASS_DESC, 2));
        assert!(h2_notice_target(&m.reader(), &h2, 0x4810_0000, None).is_none(), "aw has aj over it, not a box the caller named");
    }
}

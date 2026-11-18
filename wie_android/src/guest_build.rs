//! Which carrier build of the game this process booted, and the few guest anchors
//! that differ between builds.
//!
//! Every guest ADDRESS in this crate (`save::FN_SERIALISE`, the quickbind sites,
//! the HUD/EXP/level-cap patch sites, …) was reverse engineered from the KTF
//! `client.bin` (md5 `3792a476…`, Ver 01.00.03). The LGT `binary.mod` (Ver 01.00.07)
//! is a DIFFERENT compile — no byte-level overlap, different addresses, different
//! statics model (absolute instead of RWPI `sl`) — so on a non-KTF boot those
//! addresses point at unrelated LGT code/data.
//!
//! Two things are now separated, because they stopped being the same question:
//!
//! * [`char_anchor`] — WHERE the character record is. Known for both builds
//!   (2026-08-10, 4차): KTF reaches it through a pointer global, LGT's record IS a
//!   `.bss` static. Everything read-only (HUD gauges, quickbar scan, window/state
//!   gating, minimap) hangs off this and now works on LGT.
//! * [`guest_code_addresses_valid`] — whether the reverse-engineered CODE addresses
//!   are the ones running. Only KTF, and it stays that way until each LGT routine is
//!   found. It gates every guest CALL (`call_guest_function` at a stale address jumps
//!   into the middle of an unrelated routine and corrupts the guest) and every guest
//!   WRITE, including writes into the character record at offsets not yet verified on
//!   LGT. The byte-validation guards in front of the code patches are probabilistic;
//!   this flag is the airtight version.
//!
//! A third, [`container_data_off`], is neither: it is the carrier RUNTIME's pointer
//! convention, so it is known for every build the moment the build is known.
//!
//! The blit565 sprite-blitter perf patch is NOT gated here: it is keyed to the md5 of
//! the loaded binary in `vendor/wie/data/binary_patches.toml` and already has
//! verified LGT entries (`blit565_lgt`).
//!
//! The anchor TABLE itself lives in `guest_anchors.rs`, which is dependency-free so
//! that `wie_probe` can `include!` the shipping numbers instead of keeping a copy
//! (MIGRATION_LGT.md §3.4). Only the "which build booted" atomic is here.

use std::sync::atomic::{AtomicU8, Ordering};

// GuestBuild / CharAnchor and every per-build value, shared verbatim with wie_probe.
include!("guest_anchors.rs");
// hero2's Java-side readers and write PRODUCERS (no writes, no gates) — shared with
// wie_probe the same way, so the headless A/B runs the app's own pointer walks.
include!("hero2java.rs");

const UNKNOWN: u8 = 0;
const KTF: u8 = 1;
const LGT: u8 = 2;
const SKT: u8 = 3;
/// ★★ 영웅서기2 (KTF `client.bin1120`). A different GAME, not a carrier variant —
/// see [`GuestBuild::Hero2Ktf`]. Opening this does NOT open any hero3 address:
/// `guest_code_addresses_valid()` below still answers only to `KTF`.
const HERO2_KTF: u8 = 4;

static KIND: AtomicU8 = AtomicU8::new(UNKNOWN);

/// hero3's KTF build. ★ Nothing in *this* repo boots it — the KTF branch here is
/// 영웅서기2 ([`set_hero2_ktf`]) — but the unit tests in `items.rs` set it to exercise
/// the hero3 record shape, and deleting it would delete that coverage.
#[allow(dead_code)]
pub fn set_ktf() {
    KIND.store(KTF, Ordering::Relaxed);
}

pub fn set_lgt() {
    KIND.store(LGT, Ordering::Relaxed);
}

pub fn set_skt() {
    KIND.store(SKT, Ordering::Relaxed);
}

/// ★★ 영웅서기2 booted. This is the line that was deliberately absent for four weeks
/// (`emulator.rs`: "hero2 자체 앵커가 확보되면 … 여기서 연다"), because with
/// `kind() == None` every hero3 address stayed unreachable. It is called now because
/// hero2's OWN anchors exist — `HERO2_GUEST_RE.md` — and because the variant it sets
/// answers CLOSED to every hero3-derived accessor.
pub fn set_hero2_ktf() {
    KIND.store(HERO2_KTF, Ordering::Relaxed);
}

/// The build that booted, or `None` before the archive has been identified.
pub fn kind() -> Option<GuestBuild> {
    match KIND.load(Ordering::Relaxed) {
        KTF => Some(GuestBuild::Ktf),
        LGT => Some(GuestBuild::Lgt),
        SKT => Some(GuestBuild::Skt),
        HERO2_KTF => Some(GuestBuild::Hero2Ktf),
        _ => None,
    }
}

/// True only when the KTF build (the one all guest CODE addresses were lifted from)
/// is the one running. Every guest call and every guest write asks this first.
pub fn guest_code_addresses_valid() -> bool {
    KIND.load(Ordering::Relaxed) == KTF
}

pub fn char_anchor() -> Option<CharAnchor> {
    kind()?.char_anchor()
}

pub fn state_off() -> u32 {
    kind().map_or(0x7c, GuestBuild::state_off)
}

/// hero2's Java-side anchors (quickbar, popup, window chain, screen geometry), or `None`
/// on every other build. See [`Hero2Java`].
pub fn hero2_java() -> Option<Hero2Java> {
    kind()?.hero2_java()
}

/// hero2's one-call save (`ag.l()V`), when the build has it. See [`Hero2Save`].
pub fn hero2_save() -> Option<Hero2Save> {
    kind()?.hero2_save()
}

/// Can the host DRAW a minimap on the build that booted?
///
/// ★ Same shape of question as [`host_save_supported`], and it replaced the same kind
/// of wrong one (2026-09-19). The panel used to be gated on `char_record_is_hero3()`,
/// i.e. "does the hero3 map chain apply", which is the right test for hero3's renderer
/// and the wrong test for the feature: hero2 has its own map object and its own
/// renderer (`hero2java::h2_map`), so it can draw a panel while answering `false` to
/// the hero3 question.
pub fn minimap_supported() -> bool {
    char_record_is_hero3() || hero2_save().is_some()
}

/// Can the host save the game AT ALL on the build that booted — by either route?
///
/// ★ This is the question the HUD must ask, and it is not the same question as
/// [`save_chain_supported`] (2026-09-19). That one asks "is hero3's three-address chain
/// ported", which is a diagnostic about one mechanism; hero2 saves through a completely
/// different one and would answer `false` while saving perfectly well. Using the narrow
/// question to decide whether to draw the SAVE control is what removed the button from
/// hero2 in the first place.
pub fn host_save_supported() -> bool {
    save_chain().is_some() || hero2_save().is_some()
}

/// The per-kind gate on hero2's Java DATA writes — see [`JavaWrite`]. `false` before the
/// build is known and on every build that has no Java data.
pub fn java_write_ok(what: JavaWrite) -> bool {
    kind().is_some_and(|k| k.java_write_ok(what))
}

/// The per-method gate on hero2's JVM calls — see [`JavaCall`]. `false` before the build
/// is known and on every build without a JVM.
pub fn java_call_ok(what: JavaCall) -> bool {
    kind().is_some_and(|k| k.java_call_ok(what))
}

/// The character-record layout for the build that booted, or `None` before it is known
/// (and on builds where it was never measured). Everything that reads a gauge asks here
/// instead of carrying a constant.
pub fn char_layout() -> Option<CharLayout> {
    kind()?.char_layout()
}

/// Where the coarse screen-state word is, and what its values mean, on this build.
/// `None` keeps every mode gate closed — the same posture as an unresolvable anchor.
pub fn app_state() -> Option<AppState> {
    kind()?.app_state()
}

/// The extra field-gate bytes for this build, when it needs any. See [`FieldGate`].
pub fn field_gate() -> Option<FieldGate> {
    kind()?.field_gate()
}

/// Whether the hero3-measured CHAR offsets apply to the record this build resolves.
/// `false` before the build is known — the safe answer, since every reader that asks
/// then declines instead of decoding. See [`GuestBuild::char_record_is_hero3`].
pub fn char_record_is_hero3() -> bool {
    kind().is_some_and(GuestBuild::char_record_is_hero3)
}

/// The logical `(y, height)` of the game's OWN field HUD, when the host may paint over
/// it. `None` before the build is known, which is the safe answer: nothing gets masked.
pub fn guest_hud_band() -> Option<(u32, u32)> {
    kind()?.guest_hud_band()
}

/// hero2's viewport `− 26` → `− 0`, when located. See [`GuestBuild::viewport_full_patch`].
pub fn viewport_full_patch() -> Option<(u32, u16, u16)> {
    kind()?.viewport_full_patch()
}

/// hero2's inventory GRID SHIFT, `(site, as shipped, patched)` per halfword — empty on
/// every build that has none. See [`GuestBuild::inventory_grid_shift`].
pub fn inventory_grid_shift() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.inventory_grid_shift())
}

/// hero2's SKILL-POINT curve rewrite, `(site, as shipped, patched)` per halfword —
/// empty on every build that has none. See [`GuestBuild::skill_point_curve`].
pub fn skill_point_curve() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.skill_point_curve())
}

pub fn skill_hotkey_badges_off() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.skill_hotkey_badges_off())
}

/// hero2's COMBO SP rewrite, `(site, as shipped, patched)` — empty on builds without
/// it. See [`GuestBuild::combo_sp_gain`].
pub fn combo_sp_gain() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.combo_sp_gain())
}

/// hero2's 버서커 「근력단련」 SP term, zeroed (2026-09-26 — the skill now raises skill
/// damage through the berserker's [`SkillPowerPatch`] block instead). Empty on builds
/// without it. See [`GuestBuild::strength_training_sp_off`].
pub fn strength_training_sp_off() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.strength_training_sp_off())
}

/// hero2's 버서커 「반격태세」 counter = the character's own attack `a.a()I` instead of the
/// flat `lv·14 + 20` (2026-09-26). Empty on builds without it. See
/// [`GuestBuild::counter_stance_attack`].
pub fn counter_stance_attack() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.counter_stance_attack())
}

/// hero2's 버서커 「반격태세」 length by level — 6 s at 1, 20 s at 5 (2026-09-26); ★ 2026-10-06 25 → 75 s.
/// Empty on builds without it. See [`GuestBuild::counter_stance_duration`].
pub fn counter_stance_duration() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.counter_stance_duration())
}

/// ★ 2026-10-06: the buff skills' cooldowns at one minute — 분노태세 · 반격태세 (`bl.a[3]`/`[4]`) and the
/// beast's 빛의수호 · 빛의회복 (`bn.a(B)I`). Empty on builds without it. See
/// [`GuestBuild::buff_cooldown_minute`].
pub fn buff_cooldown_minute() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.buff_cooldown_minute())
}

/// hero2's SKILL COOLDOWN halving (#17). Empty on builds without it. See
/// [`GuestBuild::skill_cooldown_half`].
pub fn skill_cooldown_half() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.skill_cooldown_half())
}

/// hero2's fix for the shipped SKILL-window cancel crash — CLR on a PASSIVE-tab popup
/// returns instead of building a detail window for a combo stage. Empty on builds
/// without it. See [`GuestBuild::skill_popup_cancel`].
pub fn skill_popup_cancel() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.skill_popup_cancel())
}

/// ★★ 2026-10-16: the 개수 box wraps at 1 — LEFT / DOWN on 1 → the ceiling (99 when buying, the count when selling).
/// Empty on builds without it. See [`GuestBuild::qty_box_wrap`].
pub fn qty_box_wrap() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.qty_box_wrap())
}

/// ★ 2026-10-09: the crafting window's material mark without the 재료가방 index's +50 (which turned negative past cell
/// 77 of a 127-cell bag). Empty on builds without it. See [`GuestBuild::craft_material_mark`].
pub fn craft_material_mark() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.craft_material_mark())
}

/// ★ 2026-10-09: the item picker `k`'s grid 7 px right, for its page tabs. Empty on builds without it. See
/// [`GuestBuild::picker_grid_shift`].
pub fn picker_grid_shift() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.picker_grid_shift())
}

/// ★ 2026-10-09: crafting consumes the spare, never the worn copy — `(cell, shipped, resolved, patched)`. Empty on
/// builds without it. See [`GuestBuild::craft_worn_skip`].
pub fn craft_worn_skip() -> &'static [(u32, u32, u32, u32)] {
    kind().map_or(&[][..], |k| k.craft_worn_skip())
}

/// hero2's HONOUR penalty removal — weapons keep their full attack whatever the
/// player's 명예. Empty on builds without it. See [`GuestBuild::honor_penalty_off`].
pub fn honor_penalty_off() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.honor_penalty_off())
}

/// ★★ 2026-10-17 hero2: owned items take their name, price and 요구레벨 from the current item table when the save loads.
/// Empty on builds without it. See [`GuestBuild::item_table_refresh`].
pub fn item_table_refresh() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.item_table_refresh())
}

/// hero2's SPELL-CARD drop removal — the 11 % hardcoded card roll in `bd.q()V` never
/// fires. Empty on builds without it. See [`GuestBuild::card_drop_off`].
pub fn card_drop_off() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.card_drop_off())
}

/// ★★ 2026-10-18 hero2: the row immediate of the game's own pickup refusal (「빈공간이 없습니다.」 / 「재료가방이 가득 참」).
/// `None` on builds without it. See [`GuestBuild::bag_full_row_site`].
pub fn bag_full_row_site() -> Option<(u32, u16)> {
    kind().and_then(|k| k.bag_full_row_site())
}

/// hero2's 사자후 scan — the whole map instead of a screen-sized square that ran off
/// maps smaller than a landscape screen. Empty on builds without it. See
/// [`GuestBuild::roar_whole_map`].
pub fn roar_whole_map() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.roar_whole_map())
}

/// hero2's title carousel without the 네트워크창고 stop — count 6, stops 4·5 become
/// 정보이용료·게임종료 (labels move in the ASSET, `make_hero2_asset.py`). Empty on
/// builds without it. See [`GuestBuild::title_menu_no_network`].
pub fn title_menu_no_network() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.title_menu_no_network())
}

/// hero2's SKILL POWER redirect (#17), when the build has it. See [`SkillPowerPatch`].
pub fn skill_power_patch() -> Option<SkillPowerPatch> {
    kind()?.skill_power_patch()
}

/// hero2's rogue skill SP −40 % (2026-09-27). Empty on builds without it. See
/// [`GuestBuild::rogue_skill_sp_cut`].
pub fn rogue_skill_sp_cut() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.rogue_skill_sp_cut())
}

/// hero2's agility → 치명 · 회피 coefficients (2026-09-27). Empty on builds without it.
/// See [`GuestBuild::agility_coefficients`].
pub fn agility_coefficients() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.agility_coefficients())
}

/// hero2's 검기 range 3 → 5 tiles (2026-09-27). Empty on builds without it. See
/// [`GuestBuild::sword_wave_range`].
pub fn sword_wave_range() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.sword_wave_range())
}

/// hero2's knight attack from 힘 ×0.7 → ×0.85 (★ 2026-10-20). Empty on builds without it. See
/// [`GuestBuild::knight_strength_coefficient`].
pub fn knight_strength_coefficient() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.knight_strength_coefficient())
}

/// hero2's SKILL window showing the knight's seventh active — 발도 (2026-10-03). Empty on
/// builds without it. See [`GuestBuild::knight_active_seven`].
pub fn knight_active_seven() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.knight_active_seven())
}

/// ★ 2026-10-03: 사자후's cooldown table entry ×1.5. See [`GuestBuild::roar_cooldown_up`].
pub fn roar_cooldown_up() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.roar_cooldown_up())
}

/// hero2's 인내오라 → 신의가호 length / old effect / cooldown (2026-09-27). Empty on builds
/// without it. See [`GuestBuild::blessing_aura_rework`].
pub fn blessing_aura_rework() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.blessing_aura_rework())
}

/// hero2's rogue 민첩태세 → invulnerability: cast, old effect, cooldown (2026-09-27). Empty
/// on builds without it. See [`GuestBuild::agility_stance_invuln`].
pub fn agility_stance_invuln() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.agility_stance_invuln())
}

/// hero2's knight 방어의지 → 철괴: length, cooldown, no break on attack, the plain walk
/// (2026-10-05). Empty on builds without it. See [`GuestBuild::iron_wall_rework`].
pub fn iron_wall_rework() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.iron_wall_rework())
}

/// ★ 2026-10-05: the beast's 빛의회복 is a real buff (the character's buff list, its regen tick,
/// the HUD). See [`GuestBuild::beast_recovery_buff`].
pub fn beast_recovery_buff() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.beast_recovery_buff())
}

/// ★ 2026-10-07 (2): the level table never opens the berserker's 몸통치기 (retired). See [`GuestBuild::bodyslam_locked`].
pub fn bodyslam_locked() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.bodyslam_locked())
}

/// ★ 2026-10-07 (2): the monster HP delta keeps the word — the SHORT was the per-hit damage cap. See
/// [`GuestBuild::damage_cap_off`].
pub fn damage_cap_off() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.damage_cap_off())
}

/// ★★ 2026-10-16: monster HP is a 32-bit int — the other five HP stores keep the word too. See
/// [`GuestBuild::monster_hp_int`].
pub fn monster_hp_int() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.monster_hp_int())
}

/// ★ 2026-10-06: the rogue's 살상연희 (번개난타's slot) and 심안, when the build has them. See [`RoguePatch`].
pub fn rogue_patch() -> Option<RoguePatch> {
    kind()?.rogue_patch()
}

/// ★ 2026-10-08: 천공의검's picture on the character draw, when the build has it. See [`SkyDrawPatch`].
pub fn sky_draw_patch() -> Option<SkyDrawPatch> {
    kind()?.sky_draw_patch()
}

/// ★ 2026-10-06: 지력 counts ×1.5 in the beast's attack skills (the rings and the heavy three) —
/// ★ 2026-10-12 ×2, and in the 암살자 / status-7 ticks too. See [`GuestBuild::beast_int_weight`].
pub fn beast_int_weight() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.beast_int_weight())
}

/// ★ 2026-10-06: the beast's attack skills cost 20 % less SP (암살자 · the rings · the heavy three;
/// the buffs as shipped). See [`GuestBuild::beast_attack_sp_cut`].
pub fn beast_attack_sp_cut() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.beast_attack_sp_cut())
}

/// ★ 2026-10-12: the beast's attack skills cool down 20 % faster. See [`GuestBuild::beast_attack_cooldown_cut`].
pub fn beast_attack_cooldown_cut() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.beast_attack_cooldown_cut())
}

/// ★ 2026-10-12: every beast attack hits ×1.5. See [`GuestBuild::beast_attack_power`].
pub fn beast_attack_power() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.beast_attack_power())
}

/// ★ 2026-10-12: the seals' 상급 (id 3) give +15 … +20 and cost 20,000. See [`GuestBuild::seal_top_tier`].
pub fn seal_top_tier() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.seal_top_tier())
}

/// hero2's COMBAT host block (2026-09-27), when the build has it. See [`CombatPatch`].
pub fn combat_patch() -> Option<CombatPatch> {
    kind()?.combat_patch()
}

/// hero2's 맹독감염 rework (2026-09-29), when the build has it. See [`VenomPatch`].
pub fn venom_patch() -> Option<VenomPatch> {
    kind()?.venom_patch()
}

/// hero2's 야수진화제 sell price (2026-09-30), when the build has it. See [`ShopSellPatch`].
pub fn shop_sell_patch() -> Option<ShopSellPatch> {
    kind()?.shop_sell_patch()
}

/// hero2's grade line (2026-10-13), when the build has it. See [`GradeLinePatch`].
pub fn grade_line_patch() -> Option<GradeLinePatch> {
    kind()?.grade_line_patch()
}

/// hero2's boss crit floor (2026-10-10), when the build has it. See [`BossCritPatch`].
pub fn boss_crit_patch() -> Option<BossCritPatch> {
    kind()?.boss_crit_patch()
}

/// hero2's boss defence pierce (2026-10-21), when the build has it. See [`BossPiercePatch`].
pub fn boss_pierce_patch() -> Option<BossPiercePatch> {
    kind()?.boss_pierce_patch()
}

/// hero2's HELL bonus drop (2026-10-02), when the build has it. See [`HellDropPatch`].
pub fn hell_drop_patch() -> Option<HellDropPatch> {
    kind()?.hell_drop_patch()
}

/// ★ 2026-10-08 hero2's 「가디언」 mode (the template wrapper + the level-correction hooks), when the build has
/// it. See [`GuardianPatch`].
pub fn guardian_patch() -> Option<GuardianPatch> {
    kind()?.guardian_patch()
}

/// 가디언 ①: the three mode-indexed arrays four long — empty on builds without it. See
/// [`GuestBuild::guardian_mode_tables`].
pub fn guardian_mode_tables() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.guardian_mode_tables())
}

/// 가디언 ②: the round clear goes to mode 3 — empty on builds without it. See [`GuestBuild::guardian_mode_open`].
pub fn guardian_mode_open() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.guardian_mode_open())
}

/// 가디언 ③: the slot-screen / restart-notice / fee-text labels — empty on builds without it. See
/// [`GuestBuild::guardian_labels`].
pub fn guardian_labels() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.guardian_labels())
}

/// ★ 2026-10-08 밤 hero2's guardian gear options (the 13 worn-item options), when the build has it. See [`GearPatch`].
pub fn gear_patch() -> Option<GearPatch> {
    kind()?.gear_patch()
}

/// hero2's skill-changing accessories (2026-10-02), when the build has it. See [`AccessoryPatch`].
pub fn accessory_patch() -> Option<AccessoryPatch> {
    kind()?.accessory_patch()
}

/// hero2's 의상 아바타 override on the parts loader (2026-09-30), when the build has it.
/// See [`AvatarPatch`].
pub fn avatar_patch() -> Option<AvatarPatch> {
    kind()?.avatar_patch()
}

/// hero2's inventory paging patch, when the build has it. See [`InventoryPagePatch`].
pub fn inventory_page_patch() -> Option<InventoryPagePatch> {
    kind()?.inventory_page_patch()
}

/// ★★ 2026-10-20 hero2's 사용하기 → `a.g(B)V` page fix, when the build has it. See [`ItemUsePagePatch`].
pub fn item_use_page_patch() -> Option<ItemUsePagePatch> {
    kind()?.item_use_page_patch()
}

/// hero2's 경험치 배율 redirect, when the build has it. See [`ExpRatePatch`].
pub fn exp_rate_patch() -> Option<ExpRatePatch> {
    kind()?.exp_rate_patch()
}

/// hero2's 가디언 레벨 wrapper on the level-up, when the build has it. See [`GuardianLevelPatch`].
pub fn guardian_level_patch() -> Option<GuardianLevelPatch> {
    kind()?.guardian_level_patch()
}

/// hero2's 드랍률 filter, when the build has it. See [`DropRatePatch`].
pub fn drop_rate_patch() -> Option<DropRatePatch> {
    kind()?.drop_rate_patch()
}

/// hero2's round-clear counter (의상 아바타 A4), when the build has it. See [`ClearHookPatch`].
pub fn clear_hook_patch() -> Option<ClearHookPatch> {
    kind()?.clear_hook_patch()
}

/// hero2's cooldown tick counter, when the build has it. See [`CooldownClockPatch`].
pub fn cooldown_clock_patch() -> Option<CooldownClockPatch> {
    kind()?.cooldown_clock_patch()
}

/// hero2's own-icon items (엘릭서), when the build has it. See [`ItemIconPatch`].
pub fn item_icon_patch() -> Option<ItemIconPatch> {
    kind()?.item_icon_patch()
}

/// hero2's slot-screen focus (the list opens on the slot last played), when the build has it. See [`SlotFocusPatch`].
pub fn slot_focus_patch() -> Option<SlotFocusPatch> {
    kind()?.slot_focus_patch()
}

/// hero2's teleport gate (the rogue's teleports refused in the corpses' scene), when the build has it.
/// See [`TeleportGatePatch`].
pub fn teleport_gate_patch() -> Option<TeleportGatePatch> {
    kind()?.teleport_gate_patch()
}

/// ★★ 2026-10-13 「악마화」 D1: hero2's 악의 block (the beast's 대지분노 turning the character demon), when the build has it.
/// See [`DemonPatch`].
pub fn demon_patch() -> Option<DemonPatch> {
    kind()?.demon_patch()
}

/// hero2's buff-refresh redirect (a recast buff restarts its duration), when the build has it.
/// See [`BuffRefreshPatch`].
pub fn buff_refresh_patch() -> Option<BuffRefreshPatch> {
    kind()?.buff_refresh_patch()
}

/// ★ 2026-10-05: the rogue's 「일풍격」, when the build has it. See [`IlpungPatch`].
pub fn ilpung_patch() -> Option<IlpungPatch> {
    kind()?.ilpung_patch()
}

/// ★ 2026-10-07: the berserker's 분노태세 toggle numbers and 「도발」, when the build has them. See [`BerserkerPatch`].
pub fn berserker_patch() -> Option<BerserkerPatch> {
    kind()?.berserker_patch()
}

/// ★ 2026-10-07: the shipped 분노태세 effects' two halfwords — part of the berserker block's install. Empty on builds
/// without it. See [`GuestBuild::rage_shipped_off`].
pub fn rage_shipped_off() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.rage_shipped_off())
}

/// hero2's EQUIPMENT cursor-frame switch (의상 아바타 A3), when the build has it. See
/// [`EquipCursorPatch`].
pub fn equip_cursor_patch() -> Option<EquipCursorPatch> {
    kind()?.equip_cursor_patch()
}

/// hero2's EQUIPMENT cell shift (의상 아바타 A3) — empty on every build that has none. See
/// [`GuestBuild::equip_cell_shift`].
pub fn equip_cell_shift() -> &'static [(u32, u16, u16)] {
    kind().map_or(&[][..], |k| k.equip_cell_shift())
}

/// hero2's five save slots, when the build has them. See [`SaveSlotsPatch`].
pub fn save_slots_patch() -> Option<SaveSlotsPatch> {
    kind()?.save_slots_patch()
}

/// Extra rows to ask the guest for and then not show. See
/// [`GuestBuild::hud_reserved_rows`]. Zero until the build is known, so it MUST be
/// decided before the framebuffer is sized - `emulator::run` does that first thing now.
pub fn hud_reserved_rows() -> u32 {
    kind().map_or(0, GuestBuild::hud_reserved_rows)
}

/// `*(handle) + off` vs. a direct pointer, for every field holding a WIPI allocation.
/// Defaults to the KTF shape before the build is known; each caller validates its
/// hops anyway, so an unresolvable chain yields `None` rather than a bad read.
pub fn container_data_off() -> Option<u32> {
    kind().map_or(Some(8), GuestBuild::container_data_off)
}

/// The minimap's blend-suppression global, when located on this build.
pub fn minimap_noblend_global() -> Option<u32> {
    kind().and_then(GuestBuild::minimap_noblend_global)
}

/// Where the popup's per-row label allocations are listed, per build. Read-only, and
/// the single gate every cash buff has to pass — see [`GuestBuild::popup_handles`].
pub fn popup_handles() -> Option<PopupHandles> {
    kind()?.popup_handles()
}

/// How to make the game take a popup row on this build. `None` = the 퀵슬롯등록 rewrite
/// stays off, because there would be no way to commit what it offers.
pub fn popup_commit() -> Option<PopupCommit> {
    kind()?.popup_commit()
}

/// The inventory page's pre-emptor bytes to check before treating the ITEM popup as
/// live, per build. Empty before the build is known — harmless, because `popup_view`
/// cannot resolve CHAR then either. See [`GuestBuild::popup_preempt_offsets`] for why
/// only the build whose commit bypasses the game's key router carries any.
pub fn popup_preempt_offsets() -> &'static [u32] {
    kind().map_or(&[], GuestBuild::popup_preempt_offsets)
}

/// Where the auto-loot flag lives on this build, or `None` where it has not been located.
/// It is an ARRAY indexed by the save slot, and the value is a bit-7 flag, not a boolean —
/// see [`GuestBuild::autoloot_flag`] and [`AutolootFlag`].
pub fn autoloot_flag() -> Option<AutolootFlag> {
    kind()?.autoloot_flag()
}

/// `(site, original, branch)` for the game's own bottom-HUD draw, per build.
///
/// ★ This is the first guest CODE patch to be split PER ITEM rather than hidden behind
/// [`guest_code_addresses_valid`], which is the discipline MIGRATION_LGT.md §3.6 asks
/// for: the flag stays false on LGT so the routines still unported cannot fire, and
/// each item that gets reverse engineered moves into a table like this one instead of
/// loosening the flag for everything at once.
pub fn hud_bottom_patch() -> Option<(u32, u16, u16)> {
    kind()?.hud_bottom_patch()
}

/// The buff band's three x immediates, per build. Empty on a build where they have not
/// been located, which is what leaves the band where the game puts it. Same per-item
/// split as [`hud_bottom_patch`].
pub fn band_x_sites() -> &'static [(u32, u16, u8)] {
    kind().map_or(&[], GuestBuild::band_x_sites)
}

/// The target-monster box's x immediates, per build. Empty = not located.
pub fn target_box_x_sites() -> &'static [(u32, u16, i32)] {
    kind().map_or(&[], GuestBuild::target_box_x_sites)
}

/// The largest left shift the booted build's target box can take (0 = none located).
/// See [`GuestBuild::target_box_shift_max`]. Before the build is known: hero3's 211, the
/// constant every layout used until hero2 measured its own — the layout math only; the
/// WRITES are planned against the booted build's own ceiling
/// ([`GuestBuild::target_box_x_writes`]), so a stale answer here can never over-shift.
pub fn target_box_shift_max() -> u32 {
    kind().map_or(211, GuestBuild::target_box_shift_max)
}

/// The target-monster box's size in GUEST px, `(w, h)`. hero3's 90x30 reservation until
/// the build is known — the value every layout used before hero2 measured its own.
pub fn target_box_guest_size() -> (u32, u32) {
    kind().map_or((90, 30), GuestBuild::target_box_guest_size)
}

/// The writes that put the booted build's target box `shift` guest px left of where the
/// game anchors it. See [`GuestBuild::target_box_x_writes`].
pub fn target_box_x_writes(read: &impl Fn(u32, usize) -> Vec<u8>, shift: u32) -> Vec<(u32, [u8; 2])> {
    kind().map_or_else(Vec::new, |k| k.target_box_x_writes(read, shift))
}

/// May the host write the five quickbar arrays on this build? See
/// [`GuestBuild::quickbar_writes_ok`] for what "verified" means for each.
pub fn quickbar_writes_ok() -> bool {
    kind().is_some_and(GuestBuild::quickbar_writes_ok)
}

/// The game's own `skill_bind`, when located on this build. `None` puts a page restore on
/// the raw-write path instead of disabling it — see [`GuestBuild::skill_bind_fn`].
pub fn skill_bind_fn() -> Option<u32> {
    kind()?.skill_bind_fn()
}

/// Address of the kill-EXP divisor (`movs r1,#100`), per build. Same per-item split as
/// [`hud_bottom_patch`].
pub fn exp_divisor_site() -> Option<u32> {
    kind()?.exp_divisor_site()
}

/// The game's own `checkLevelUp` (thumb bit included), per build — the guest call the
/// 천상의 영약 cash item makes. Same per-item split as [`hud_bottom_patch`].
pub fn level_up_fn() -> Option<u32> {
    kind()?.level_up_fn()
}

/// The network-warehouse deposit grade gate `(site, orig, patched)`, per build. Same
/// per-item split as [`hud_bottom_patch`]. Un-gates 영웅-이상 (cash) equipment deposit.
pub fn warehouse_grade_gate() -> Option<(u32, u16, u16)> {
    kind()?.warehouse_grade_gate()
}

/// Where and HOW to relax the equip-level check, per build. Same per-item split as
/// [`hud_bottom_patch`]. The LGT site is located but deliberately ships `None` until
/// its verification run — see the table entry for the rule this enforces.
pub fn equip_relief_site() -> Option<EquipReliefSite> {
    kind()?.equip_relief_site()
}

/// The game's own "연결이 끊어졌습니다" modal, per build. `None` = the host can only toast,
/// and the player stays where they are — see [`NetDisconnectModal`] for why nothing
/// simpler works. Same per-item split as [`hud_bottom_patch`].
pub fn net_disconnect_modal() -> Option<NetDisconnectModal> {
    kind()?.net_disconnect_modal()
}

/// The full per-build save chain, when ported — see [`GuestBuild::save_chain`] for the
/// addresses and each build's scratch proof. Same per-item split as [`hud_bottom_patch`].
pub fn save_chain() -> Option<SaveChain> {
    kind()?.save_chain()
}

/// The chain converted to `save::Chain` — the mirror `save.rs` can name while staying
/// dependency-free for `save_selftest.rs`. One conversion point, here, so the two
/// structs cannot drift apart in more than one place.
pub fn save_chain_fns() -> Option<crate::save::Chain> {
    let c = save_chain()?;
    Some(crate::save::Chain {
        fn_serialise: c.fn_serialise,
        fn_slot_summary: c.fn_slot_summary,
        fn_write_record: c.fn_write_record,
        scratch: c.scratch,
    })
}

/// Can the host drive the game's own save primitives on the build that booted?
///
/// ★ DIAGNOSTIC ONLY — it must not hide or disable anything (2026-08-11). An earlier
/// pass this same day used it to remove the SAVE control on LGT, and that was the wrong
/// trade: a feature that already shipped does not get taken away because a carrier port
/// is unfinished. The fix is to finish MIGRATION_LGT.md §4-3.
///
/// It exists because a refusal has two very different causes and a device log has to
/// distinguish them: "the place forbids saving" (the game's own AREA byte) versus "this
/// build's save chain was reverse engineered from a different compile". Without the
/// second one the symptom is only `save: queued Manual retry expired unfulfilled`,
/// which reads like the first.
///
/// §4-3 got its per-build table on 2026-08-11 (9차): this now follows
/// [`GuestBuild::save_chain`], so the LGT port turning real is what flipped it — not a
/// loosened [`guest_code_addresses_valid`].
pub fn save_chain_supported() -> bool {
    save_chain().is_some()
}

pub fn name() -> &'static str {
    kind().map_or("unknown", GuestBuild::name)
}

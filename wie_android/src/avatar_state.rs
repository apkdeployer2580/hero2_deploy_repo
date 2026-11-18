// ★★ 의상 아바타 A2 (2026-09-30) — 캐릭터별 의상 상태와 그 저장 (HERO2_AVATAR_PLAN.md §4 · §6 · §7).
// ★★ A3c (2026-10-01) — 옷장 네 칸: 투구 · 갑옷 · 신발 · 무기 (HERO2_AVATAR_PLAN.md §9 「A3c」).
// ★★ A4 (2026-10-01) — 다섯째 칸 보조손 · 회차 클리어 보상 (HERO2_AVATAR_PLAN.md §9 「A4」).
// ★★ A5 (2026-10-01) — 기본 지급 세트(클레르 202) · HELL 보상 둘(수영장 파티 203 · 할로윈 204) · 보상의 「난이도 이상」
//    조건 = 그 회차에 완료한 퀘스트(게임 자신의 달성도)의 95 % 를 그 난이도 이상에서 (HERO2_AVATAR_PLAN.md §9 「A5」).
// ★★ 배포 준비 (2026-10-02) — HELL 의 둘이 자리를 바꿨다(할로윈 = EASY 이상 · 수영장 파티 = NORMAL 이상) · 이 빌드
//    전에 깬 회차는 세이브가 말하는 만큼 EASY 클리어로 쳐서 그 보상을 준다([`cleared_before`] · [`AvatarBook::grant_earned`]).
//
// ── WHAT THIS HOLDS ──────────────────────────────────────────────────────────
// A character OWNS avatar ITEMS. A costume SET (`costumes/<n>_<name>/`, named by its SHEET
// NUMBER n — `hero/<part>/<n>`) is up to five items, one per avatar SLOT:
//   0 투구 head (part 0) · 1 갑옷 armour (body + arms, parts 1 2) · 2 신발 shoes (legs, part 3)
//   · 3 무기 weapon (part 6) · 4 보조손 off-hand (part 7 — the knight's shield, the rogue's
//   second dagger; a berserker draws none)
// — the cells of the equipment window's avatar side. Getting a set gives each of its items
// (user, 2026-10-01: 「세트를 획득하면 이안의 머리, 이안의 낡은 옷, 이안의 낡은 신발, 이안의 낡은
// 대검 등 획득하지만, 각각 따로따로 아바타 장착이 가능하다」), and each slot WEARS at most one
// instance of its own kind — so a head from one set and the clothes of another combine
// (「이안의 머리 … 율리케의 웨딩드레스 … 따로따로 조합」). The same item can be owned twice (two
// receipts: e.g. one from NORMAL, one from HARDCORE); the receipts stop one reward from being
// paid twice. Definitions (names, sets, the sheets themselves) live in the asset; this holds only
// what the character has, so a new costume needs no code here.
//
// ── WHY IT IS BOUND TO `/g<slot>` ───────────────────────────────────────────
// The user's rule (plan §1): what was gained or worn since the last save is gone after dying
// and loading again — exactly like the game's own progress. So the state is part of the
// save: committed when the game saves, handed back when that save is loaded. Two facts from
// the game decide what "the save" is (HERO2_GUEST_RE.md §8.35.9):
//   * `ag.l()V` — the only full save — writes `/g<slot>` (character · quickbar · materials)
//     FIRST and `/n<slot>` (bags) second, and nothing else writes `/g`;
//   * `/n<slot>` is ALSO written alone (`b.a(BB)V`, `y.*`) — binding to it would unbind the
//     state whenever the bags are saved by themselves.
// So a committed record carries `bind` = FNV-1a 64 of the `/g<slot>` it was committed with,
// and loading a slot hands the record back only when the `/g<slot>` on disk hashes to it. A
// save the record does not match — an older vault generation without a record, a save copied
// in from elsewhere, a save from before this feature — loads as LEGACY, empty: the plan's
// "미래 아이템이 과거 세이브에 남지 않는다". ★ 2026-10-02 the plan's other half, "소급 지급 없음", was
// turned round by the user (배포 준비): what the SAVE ITSELF says was cleared — its mode and 달성도 —
// is paid on loading it ([`cleared_before`], [`AvatarBook::grant_earned`]), so a legacy save gets
// the rewards its own progress proves, and never another save's items.
//
// ── THE CRASH WINDOW, AND THE JOURNAL ────────────────────────────────────────
// The game writes `/g` inside one emulator tick; the host commits right after it. A process
// killed between the two would leave a new `/g` beside the old record, and the new save would
// load as LEGACY — its costumes gone. So at the FIRST edge of a save (the vault's
// `before_overwrite`, before the game truncates `/g`) the host writes a PENDING record: the
// state being saved, keyed by `base` = the hash of the `/g` about to be replaced. Loading then
// resolves ([`resolve_load`]):
//   committed.bind == hash(g)                                   → that state (the normal case)
//   pending.base == committed.bind (or nothing committed yet),
//   hash(g) != base and g is complete                           → the save happened and the
//                                                                 commit did not: the pending
//                                                                 state, committed now
//   anything else                                               → LEGACY (empty)
//
// DEPENDENCY-FREE (std only) and edition 2021 — `avatar_state_selftest.rs` and the probe
// `include!` it. Plain `//` comments: an `include!`d file cannot carry `//!`.

/// The record format this build writes. It reads this one and every older one: schema 1 (A2 —
/// one instance was a whole costume) and schema 2 (A3c — the weapon item drew the off-hand too)
/// are migrated on read ([`decode_record`]).
pub const AVATAR_SCHEMA: u32 = 3;

/// What a record is keyed by: the FNV-1a 64 of a `/g<slot>` file. `0` = none (the hash
/// never returns it).
pub type Bind = u64;
pub const UNBOUND: Bind = 0;

// ── the four avatar slots ────────────────────────────────────────────────────

/// How many avatar slots — the equipment window's avatar cells. ★ A4: five (the off-hand got
/// its own — user, 2026-10-01: *"나이트는 방패까지, 로그는 보조단검까지"*).
pub const SLOTS: usize = 5;
#[allow(dead_code)] // named for the selftest and the probe; the app indexes slots by number
pub const SLOT_HEAD: u8 = 0;
pub const SLOT_ARMOUR: u8 = 1;
#[allow(dead_code)] // ditto
pub const SLOT_SHOES: u8 = 2;
pub const SLOT_WEAPON: u8 = 3;
pub const SLOT_OFFHAND: u8 = 4;
/// The slots' words in a record and in the jar's catalog (`hero/costume.txt`).
pub const SLOT_KEYS: [&str; SLOTS] = ["head", "armor", "shoes", "weapon", "offhand"];
/// The parts each slot's item draws: the head · the clothes (the game's gloves have no avatar
/// cell — sleeves and gloves are the clothes') · the legs · the weapon · the off-hand. The
/// override is built from [`SLOT_OF_PART`]; this is its reading by slot (the selftest holds the
/// two consistent).
#[allow(dead_code)]
pub const SLOT_PARTS: [&[u8]; SLOTS] = [&[0], &[1, 2], &[3], &[6], &[7]];
/// The slot that draws each of [`COSTUME_PARTS`], in that order (`hero2java` repeats it as
/// `H2_AV_PART_SLOT`; the app and the probe assert the two equal).
pub const SLOT_OF_PART: [u8; 6] = [1, 1, 2, 0, 3, 4];

/// A slot's record word — `None` for a number that is not a slot.
pub fn slot_key(slot: u8) -> Option<&'static str> {
    SLOT_KEYS.get(usize::from(slot)).copied()
}

/// The slot a record word names.
#[allow(dead_code)] // the selftest's; the app reads slots as numbers
pub fn slot_of_key(key: &str) -> Option<u8> {
    SLOT_KEYS.iter().position(|k| *k == key).map(|i| i as u8)
}

/// One avatar item the character owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    /// Unique within the character, never reused (`AvatarState::next_id`).
    pub id: u32,
    /// The set = its sheet number (`hero/<part>/<avatar>`).
    pub avatar: u16,
    /// Which avatar slot it is worn in (and so which parts it draws) — `SLOT_*`.
    pub slot: u8,
    /// The game mode it was gained in (`ag.i:B`: 0 NORMAL · 1 HARDCORE · 2 HELL).
    pub mode: u8,
    /// The receipt that paid it (0 = none — granted by a test or a tool).
    pub receipt: u32,
}

/// One reward paid out — the key that stops it being paid twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub id: u32,
    /// Which reward (the reward definition's number, A4).
    pub source: u32,
    pub mode: u8,
    /// A repeatable reward's occurrence (0 for once-per-mode rewards).
    pub occurrence: u32,
}

/// Everything one character has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarState {
    /// Issued when the character is created (a new game), kept for its whole life —
    /// including the next modes, which the game plays on the same character.
    pub character: u64,
    /// The next instance / receipt id.
    pub next_id: u32,
    pub items: Vec<Instance>,
    /// The instance each slot wears (`SLOT_*` order).
    pub wear: [Option<u32>; SLOTS],
    pub receipts: Vec<Receipt>,
    /// `(source, mode)` already complete when the reward or this feature arrived — never
    /// paid for retroactively (plan §6). Read and written back for the records' sake; NO build
    /// writes it: A4 never did, and ★ 2026-10-02 the user's rule is the opposite (a save's own
    /// earlier clears are paid — [`AvatarBook::grant_earned`]).
    pub done: Vec<(u32, u8)>,
    /// ★ A5: this round's quests, one mark each — [`QUEST_OPEN`] (not done as the host last saw
    /// it), the difficulty it was done at (0 EASY · 1 NORMAL · 2 HARD), or [`QUEST_UNKNOWN`]. The
    /// run's progress for a reward that asks for a difficulty ([`AvatarState::run_progress`]).
    pub quests: [u8; QUESTS],
    /// ★★ 2026-10-22 가디언 레벨: the character's GL (1..=250 at LV 99; 0 = below 99, or a record from before GL). The
    /// level-up's block keeps it in the guest while the character is in the world (`guest_anchors::GuardianLevelPatch`);
    /// the host copies it here as it moves, so a save carries the GL with the stat points the same routine gave, and a
    /// reload without saving rolls both back. One line `gl <n>` (only when > 0 — schema 3 is unchanged, an older build
    /// keeps the line verbatim as unknown).
    pub gl: u16,
    /// Lines of a record this build does not know (a newer writer), written back verbatim.
    pub unknown: Vec<String>,
}

// ── ★ A5: the run's progress, by difficulty ──────────────────────────────────
//
// User (2026-10-01): *"노말난이도 이상은 난이도 노말인채로 진행도가 95프로 이상 진행되어야해. 이지 난이도로 전부
// 진행하고 마지막에 난이도만 바꿔서 클리어하면 획득 못해."* The game's own measure of a round's progress is its QUESTS:
// `ag.a()B` @0x115008 (the 달성도 `ag.y()V` adds to `ag.j` at every clear) counts the quest slots 0..=50 whose state
// (`ag.a(I)B` @0x111ba8 — two bits of `ag.r:[B` @0x193a5c) is 2 = done, and every world entry clears them
// (`ag.t()V` @0x111e34, event 9 — a new game, a load before the save's own come back, and the next round). So
// the host marks each quest with the 난이도 (`crate::difficulty()`, a host setting) it was done at, the marks
// ride in this record (bound to the save, so a reload hands back the marks of what it reloads), and a reward
// that asks for a difficulty pays only when [`DIFFICULTY_SHARE_PCT`] of the round's done quests were done at
// it or above. A quest done before the host could see — a save from before this build, a load without a
// record — is [`QUEST_UNKNOWN`]: in the total, never qualified (fail closed, the "소급 없음" rule).

/// How many quest slots the game counts (`ag.a()B`: 0..=50).
pub const QUESTS: usize = 51;
/// A quest not done (as the host last saw the game's quest states).
pub const QUEST_OPEN: u8 = 0xff;
/// A quest done before the host could see at which difficulty.
pub const QUEST_UNKNOWN: u8 = 0xfe;
/// The difficulties a quest can be marked with (`hero2java::H2_OPT_DIFFICULTY_VALUES`).
pub const DIFFICULTIES: u8 = 3;

/// The 난이도 in the game's own words (`hero2java::H2_OPT_DIFFICULTY_VALUES`), for the log.
pub fn difficulty_label(d: u8) -> &'static str {
    match d {
        0 => "EASY",
        1 => "NORMAL",
        2 => "HARD",
        _ => "?",
    }
}

// `allow(dead_code)`: some of these are driven only by the probe, the costume window (A3) or
// the rewards (A4) — not every build calls every one.
#[allow(dead_code)]
impl AvatarState {
    /// A new character: owns nothing, wears nothing.
    pub fn fresh(character: u64) -> Self {
        Self { character, next_id: 1, items: Vec::new(), wear: [None; SLOTS], receipts: Vec::new(), done: Vec::new(), quests: [QUEST_OPEN; QUESTS], gl: 0, unknown: Vec::new() }
    }

    /// ★ A5: the game's quest states as the host just read them (`done[i]` = quest `i` done), seen at
    /// difficulty `difficulty`. A quest newly done is marked with it; one no longer done (the next
    /// round cleared them all, a reload of an earlier save) loses its mark. `baseline` = the first
    /// look after a load: a done quest the record has no mark for was done before anyone could see —
    /// [`QUEST_UNKNOWN`] — and no mark is taken away (the look that matters is the next one). Returns
    /// the quests newly marked with `difficulty`.
    pub fn observe_quests(&mut self, done: &[bool; QUESTS], difficulty: u8, baseline: bool) -> Vec<usize> {
        let d = difficulty.min(DIFFICULTIES - 1);
        let mut newly = Vec::new();
        for (i, (&is_done, mark)) in done.iter().zip(self.quests.iter_mut()).enumerate() {
            match (is_done, *mark == QUEST_OPEN) {
                (true, true) if baseline => *mark = QUEST_UNKNOWN,
                (true, true) => {
                    *mark = d;
                    newly.push(i);
                }
                (false, false) if !baseline => *mark = QUEST_OPEN,
                _ => {}
            }
        }
        newly
    }

    /// ★ A5: this round's progress for a reward that asks for `min_difficulty`: `(done at it or
    /// above, done)` — every marked quest counts in the total, an unknown one never above.
    pub fn run_progress(&self, min_difficulty: u8) -> (u32, u32) {
        let done = self.quests.iter().filter(|m| **m != QUEST_OPEN).count() as u32;
        let at = self.quests.iter().filter(|m| **m < DIFFICULTIES && **m >= min_difficulty).count() as u32;
        (at, done)
    }

    /// ★ A5: a round was cleared — the next starts with every quest open (the game clears them too).
    pub fn reset_run(&mut self) {
        self.quests = [QUEST_OPEN; QUESTS];
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// Add one owned item — set `avatar`'s `slot` item; returns its instance id.
    pub fn grant(&mut self, avatar: u16, slot: u8, mode: u8, receipt: u32) -> u32 {
        let id = self.take_id();
        self.items.push(Instance { id, avatar, slot, mode, receipt });
        id
    }

    /// Add a whole set: one instance per slot in `slots` (the set's items — the catalog says
    /// which). Returns the ids in `slots` order.
    pub fn grant_set(&mut self, avatar: u16, slots: &[u8], mode: u8, receipt: u32) -> Vec<u32> {
        slots.iter().map(|&slot| self.grant(avatar, slot, mode, receipt)).collect()
    }

    /// Does the character own set `avatar`'s `slot` item at least once?
    pub fn owns(&self, avatar: u16, slot: u8) -> bool {
        self.items.iter().any(|i| i.avatar == avatar && i.slot == slot)
    }

    /// ★ The one-shot grant (the boot marker): every `(avatar, slot)` item of `items` not owned
    /// yet, once — owning it already is enough, so running it twice adds nothing. Nothing is
    /// worn. Returns what was added.
    pub fn grant_missing(&mut self, items: &[(u16, u8)], mode: u8) -> Vec<(u16, u8)> {
        let mut added = Vec::new();
        for &(avatar, slot) in items {
            if usize::from(slot) < SLOTS && !self.owns(avatar, slot) && !added.contains(&(avatar, slot)) {
                self.grant(avatar, slot, mode, 0);
                added.push((avatar, slot));
            }
        }
        added
    }

    /// Was this reward paid already (its receipt, or a completion the character had before the
    /// reward existed)?
    pub fn paid(&self, source: u32, mode: u8, occurrence: u32) -> bool {
        self.receipts.iter().any(|r| r.source == source && r.mode == mode && r.occurrence == occurrence) || self.done.contains(&(source, mode))
    }

    /// ★ 2026-10-09: has this character been paid a GUARDIAN clear — a receipt of one of GUARDIAN's rewards? The save's
    /// bytes cannot show one (its 달성도 weight is 0, [`MODE_GUARDIAN`]); the record bound to that save can. Every such
    /// reward asks for EASY or above, so a receipt proves a clear at EASY at least ([`AvatarBook::grant_earned`]).
    pub fn cleared_guardian(&self) -> bool {
        self.receipts.iter().any(|r| r.mode == MODE_GUARDIAN && CLEAR_REWARDS.iter().any(|c| c.mode == MODE_GUARDIAN && c.source == r.source))
    }

    /// ★ 2026-10-16 「보스 레이드」 (user: *"외형이 바뀌는 기준은 가디언 모드 하드난이도를 클리어하면 그떄만 바뀌는거야. 클리어가 되지않으면
    /// 바뀌면안돼"*): a GUARDIAN clear at HARD or above — the receipt of 달빛 엘프 209 (`CLEAR_REWARD_SOURCE_GUARDIAN_HARD`), which
    /// [`AvatarBook::clear_rewards`] writes only when the round met HARD ([`difficulty_met`]). It rides the record the way every
    /// receipt does: committed with the save that carries it, taken back by a reload that did not save. What the temple's
    /// raid gate asks ([`raid_gate_for_save`]).
    /// ★ 2026-10-18 (user: *"보스레이드 해금도 95%이상 hard 로 진행해야 해금이 되야되는게 맞아"*): 209 alone, on purpose — the
    /// gate takes the costume's 95 % rule, not the 난이도 at the clear.
    pub fn cleared_guardian_hard(&self) -> bool {
        self.receipts.iter().any(|r| r.mode == MODE_GUARDIAN && r.source == CLEAR_REWARD_SOURCE_GUARDIAN_HARD)
    }

    /// Record a reward as paid — `None` when it already was (the same receipt, or a
    /// completion the character had before the reward existed).
    pub fn receipt(&mut self, source: u32, mode: u8, occurrence: u32) -> Option<u32> {
        if self.paid(source, mode, occurrence) {
            return None;
        }
        let id = self.take_id();
        self.receipts.push(Receipt { id, source, mode, occurrence });
        Some(id)
    }

    /// Wear an owned instance in `slot`, or nothing there. `false` (and no change) for a slot
    /// that is not one, or an instance not owned or of another slot's kind.
    pub fn wear(&mut self, slot: u8, id: Option<u32>) -> bool {
        let Some(w) = self.wear.get_mut(usize::from(slot)) else { return false };
        match id {
            None => {
                *w = None;
                true
            }
            Some(id) if self.items.iter().any(|i| i.id == id && i.slot == slot) => {
                *w = Some(id);
                true
            }
            Some(_) => false,
        }
    }

    /// The instance `slot` wears, if it is still owned and of that slot's kind.
    pub fn worn_instance(&self, slot: u8) -> Option<&Instance> {
        let id = (*self.wear.get(usize::from(slot))?)?;
        self.items.iter().find(|i| i.id == id && i.slot == slot)
    }

    /// The set each slot wears (`SLOT_*` order).
    pub fn worn_avatars(&self) -> [Option<u16>; SLOTS] {
        let mut out = [None; SLOTS];
        for (s, o) in out.iter_mut().enumerate() {
            *o = self.worn_instance(s as u8).map(|i| i.avatar);
        }
        out
    }

    /// How many items of set `avatar` the character owns (all slots).
    pub fn count(&self, avatar: u16) -> usize {
        self.items.iter().filter(|i| i.avatar == avatar).count()
    }
}

// ── the record ───────────────────────────────────────────────────────────────

/// Which of the two per-slot records a text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    /// `avatar/<slot>` — keyed by the `/g` it was committed with (`bind`).
    Committed,
    /// `avatar/<slot>.pending` — keyed by the `/g` the save is replacing (`base`).
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordKind,
    pub key: Bind,
    pub state: AvatarState,
}

const HEADER: &str = "hero2-avatar";

/// The text form: one fact per line, so a record read by hand (or diffed in a bug report)
/// says what the character has.
pub fn encode_record(kind: RecordKind, key: Bind, s: &AvatarState) -> String {
    let mut out = format!("{HEADER} {AVATAR_SCHEMA}\n");
    out += &format!("{} {key:016x}\n", if kind == RecordKind::Committed { "bind" } else { "base" });
    out += &format!("char {:016x}\n", s.character);
    out += &format!("next {}\n", s.next_id);
    for i in &s.items {
        out += &format!("item {} {} {} {} {}\n", i.id, i.avatar, i.mode, i.receipt, i.slot);
    }
    let worn: Vec<String> = s.wear.iter().map(|w| w.map_or_else(|| "-".to_string(), |id| id.to_string())).collect();
    out += &format!("wear {}\n", worn.join(" "));
    for r in &s.receipts {
        out += &format!("recv {} {} {} {}\n", r.id, r.source, r.mode, r.occurrence);
    }
    for (source, mode) in &s.done {
        out += &format!("done {source} {mode}\n");
    }
    // ★ A5: the round's quest marks — `-` open · `0` `1` `2` the difficulty · `?` unknown. Only when
    // one is marked (an older build keeps the line verbatim as unknown — schema 3 is unchanged).
    if s.quests.iter().any(|m| *m != QUEST_OPEN) {
        let marks: String = s.quests.iter().map(|m| quest_mark_char(*m)).collect();
        out += &format!("quest {marks}\n");
    }
    // ★★ 2026-10-22 가디언 레벨.
    if s.gl > 0 {
        out += &format!("gl {}\n", s.gl);
    }
    for line in &s.unknown {
        out += line;
        out += "\n";
    }
    out
}

/// ★ A5: a quest mark's character in a record.
fn quest_mark_char(m: u8) -> char {
    match m {
        QUEST_OPEN => '-',
        d if d < DIFFICULTIES => char::from(b'0' + d),
        _ => '?',
    }
}

/// ★ A5: the mark a record's character stands for.
fn quest_mark_of(c: char) -> u8 {
    match c {
        '-' => QUEST_OPEN,
        '0'..='9' if (c as u8 - b'0') < DIFFICULTIES => c as u8 - b'0',
        _ => QUEST_UNKNOWN,
    }
}

/// A record's text. ★ A3c: a schema-1 record (A2/A3 — one instance was a WHOLE costume, worn
/// whole) is migrated: each such instance becomes one instance per slot — the armour keeps its
/// id (A2/A3 named the costume after its clothes), the others get new ones — and a worn one is
/// worn in every slot, so the character looks and owns exactly what it did. ★ A4: a schema-2
/// weapon item (weapon + off-hand) becomes a weapon item and an off-hand item.
pub fn decode_record(text: &str) -> Result<Record, String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let head = lines.next().ok_or("empty record")?;
    let mut hw = head.split_whitespace();
    if hw.next() != Some(HEADER) {
        return Err(format!("not an avatar record: {head:?}"));
    }
    let schema: u32 = hw.next().and_then(|v| v.parse().ok()).ok_or("no schema")?;
    if schema > AVATAR_SCHEMA {
        return Err(format!("schema {schema} is newer than this build's {AVATAR_SCHEMA}"));
    }
    let mut kind = None;
    let mut key = UNBOUND;
    let mut s = AvatarState::fresh(0);
    let mut have_char = false;
    // Schema 1: whole-costume instances `(id, avatar, mode, receipt)` and a single `wear`.
    let mut whole: Vec<(u32, u16, u8, u32)> = Vec::new();
    let mut whole_wear: Option<u32> = None;
    for line in lines {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| -> Result<u64, String> { f.get(i).and_then(|v| v.parse::<u64>().ok()).ok_or_else(|| format!("bad line {line:?}")) };
        let hex = |i: usize| -> Result<u64, String> { f.get(i).and_then(|v| u64::from_str_radix(v, 16).ok()).ok_or_else(|| format!("bad line {line:?}")) };
        let id_or_none = |v: &str| -> Result<Option<u32>, String> {
            if v == "-" {
                Ok(None)
            } else {
                v.parse::<u32>().map(Some).map_err(|_| format!("bad line {line:?}"))
            }
        };
        match f[0] {
            "bind" | "base" => {
                if kind.is_some() {
                    return Err("two keys".into());
                }
                kind = Some(if f[0] == "bind" { RecordKind::Committed } else { RecordKind::Pending });
                key = hex(1)?;
            }
            "char" => {
                s.character = hex(1)?;
                have_char = true;
            }
            "next" => s.next_id = u32::try_from(num(1)?).map_err(|_| "next out of range")?,
            "item" => {
                let id = u32::try_from(num(1)?).map_err(|_| "item id")?;
                let avatar = u16::try_from(num(2)?).map_err(|_| "item avatar")?;
                let mode = u8::try_from(num(3)?).map_err(|_| "item mode")?;
                let receipt = u32::try_from(num(4)?).map_err(|_| "item receipt")?;
                match f.len() {
                    5 => whole.push((id, avatar, mode, receipt)),
                    6 => {
                        let slot = u8::try_from(num(5)?).ok().filter(|v| usize::from(*v) < SLOTS).ok_or("item slot")?;
                        s.items.push(Instance { id, avatar, slot, mode, receipt });
                    }
                    _ => return Err(format!("bad line {line:?}")),
                }
            }
            "wear" => match f.len() {
                2 => whole_wear = id_or_none(f[1])?,
                // Schema 2's four slots, or this schema's five.
                n if n == 1 + 4 || n == 1 + SLOTS => {
                    for (slot, v) in f[1..].iter().enumerate() {
                        s.wear[slot] = id_or_none(v)?;
                    }
                }
                _ => return Err(format!("bad line {line:?}")),
            },
            "recv" => s.receipts.push(Receipt {
                id: u32::try_from(num(1)?).map_err(|_| "recv id")?,
                source: u32::try_from(num(2)?).map_err(|_| "recv source")?,
                mode: u8::try_from(num(3)?).map_err(|_| "recv mode")?,
                occurrence: u32::try_from(num(4)?).map_err(|_| "recv occurrence")?,
            }),
            "done" => s.done.push((u32::try_from(num(1)?).map_err(|_| "done source")?, u8::try_from(num(2)?).map_err(|_| "done mode")?)),
            // ★ A5: lenient — a character beyond the slots is ignored, a missing one is open, one this
            // build does not know is unknown (a bad mark must not cost the character its costumes).
            "quest" => {
                for (mark, ch) in s.quests.iter_mut().zip(f.get(1).copied().unwrap_or("").chars()) {
                    *mark = quest_mark_of(ch);
                }
            }
            // ★★ 2026-10-22 가디언 레벨 — lenient like the quests: a bad number is no GL (the arm plan then treats a
            // LV 99 character as met at 99), never a lost record.
            "gl" => s.gl = f.get(1).and_then(|v| v.parse::<u16>().ok()).unwrap_or(0),
            _ => s.unknown.push(line.to_string()),
        }
    }
    let kind = kind.ok_or("no bind/base line")?;
    if !have_char {
        return Err("no char line".into());
    }
    // Ids must stay unique after a grant: never below every id already used.
    let top = s.items.iter().map(|i| i.id).chain(whole.iter().map(|w| w.0)).chain(s.receipts.iter().map(|r| r.id)).max().unwrap_or(0);
    if s.next_id <= top {
        s.next_id = top.saturating_add(1);
    }
    // ★ A4: a schema-2 weapon item drew the off-hand too (one item for parts 6 and 7) — it becomes
    // two: the weapon (its own id) and the off-hand (a new id, worn when the weapon was), so the
    // character owns and looks exactly as before. A berserker's off-hand item is never shown (it
    // has no such cell). Before the schema-1 migration, whose instances already come per slot.
    if schema <= 2 {
        let weapons: Vec<Instance> = s.items.iter().filter(|i| i.slot == SLOT_WEAPON).cloned().collect();
        for w in weapons {
            let id = s.take_id();
            s.items.push(Instance { id, avatar: w.avatar, slot: SLOT_OFFHAND, mode: w.mode, receipt: w.receipt });
            if s.wear[usize::from(SLOT_WEAPON)] == Some(w.id) {
                s.wear[usize::from(SLOT_OFFHAND)] = Some(id);
            }
        }
    }
    // The schema-1 migration (see above).
    for (id, avatar, mode, receipt) in whole {
        let worn = whole_wear == Some(id);
        for slot in 0..SLOTS as u8 {
            let sid = if slot == SLOT_ARMOUR { id } else { s.take_id() };
            s.items.push(Instance { id: sid, avatar, slot, mode, receipt });
            if worn {
                s.wear[usize::from(slot)] = Some(sid);
            }
        }
    }
    Ok(Record { kind, key, state: s })
}

// ── the save it belongs to ───────────────────────────────────────────────────

/// FNV-1a 64 — dependency-free and stable; never returns [`UNBOUND`].
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    if h == UNBOUND {
        1
    } else {
        h
    }
}

/// The keys `bg.a([B[B)[B` encrypts the five `/g` blobs with (`ag.<clinit>` constants, the
/// same table as `tools_re/h2save.py`).
const G_KEY_N: [u8; 6] = [0x05, 0x47, 0x18, 0x3B, 0x2B, 0x10];
const G_KEY_O: [u8; 6] = [0x0E, 0x1A, 0x48, 0x0A, 0x1C, 0x62];
const G_KEY_P: [u8; 6] = [0x43, 0x0C, 0x23, 0x44, 0x5B, 0x2B];
const G_KEYS: [[u8; 6]; 5] = [G_KEY_N, G_KEY_O, G_KEY_P, G_KEY_N, G_KEY_O];
/// Bytes after the five blobs — `ag.g(B)V` ends every `/g` with four.
const G_TAIL: usize = 4;

/// Is this `/g<slot>` a whole save — five `[u16 BE len][cipher + checksum]` blobs whose
/// checksum bytes hold (`bg.a` appends the sum of the key bytes it used, a function of the
/// length alone, so no decryption is needed), then exactly the four tail bytes? A file the
/// game is still writing, or one cut short, is not.
pub fn g_complete(g: &[u8]) -> bool {
    let mut p = 0usize;
    for key in G_KEYS.iter() {
        if p + 2 > g.len() {
            return false;
        }
        let n = usize::from(u16::from_be_bytes([g[p], g[p + 1]]));
        p += 2;
        if n == 0 || p + n > g.len() {
            return false;
        }
        let body = n - 1;
        let sum: u32 = (0..body).map(|i| u32::from(key[(i + 1) % key.len()])).sum();
        if u32::from(g[p + body]) != sum & 0xff {
            return false;
        }
        p += n;
    }
    p + G_TAIL == g.len()
}

/// ★ A4: the class (0 버서커 · 1 나이트 · 2 로그) of the character a `/g<slot>` holds — byte 4 of
/// its first blob, the stage blob (`bf.a()[B`: map · x · y · facing · class, `tools_re/h2save.py`),
/// decrypted the way `bg.a` encrypts it (`out[i] = in[i] ^ key[(i + 1) % 6]`). `None` for a
/// file that is not a whole save, or a value that is not a class. What the boot grant reads to
/// give a character only its own class's items.
pub fn g_class(g: &[u8]) -> Option<u8> {
    if !g_complete(g) {
        return None;
    }
    // The blob's length counts its checksum byte: the class needs five bytes before it.
    if usize::from(u16::from_be_bytes([g[0], g[1]])) < 5 + 1 {
        return None;
    }
    let c = g[2 + 4] ^ G_KEY_N[(4 + 1) % G_KEY_N.len()];
    (c < 3).then_some(c)
}

/// The words a log line names a class with.
pub fn class_label(class: Option<u8>) -> &'static str {
    match class {
        Some(0) => "berserker",
        Some(1) => "knight",
        Some(2) => "rogue",
        _ => "class unknown",
    }
}

// ── ★ 2026-10-02: what a save says was cleared before ─────────────────────────
//
// User (배포 준비): *"apk를 업데이트한 이용자들이 각 조건에 맞게 아바타를 가지고있어야해. 예를들어서 이미 헬모드로 클리어를 한
// 사용자들은 헬모드 이지모드 클리어한 조건으로 간주하고 클레르, 이안,율리케, 할로윈 해골 아바타를 전부 가지고있어야해."* A save
// from before the rewards (or from before this build) holds no receipt, but it holds the game's own account of how far
// the character went — two bytes of `/g`'s third blob (`ag.b()[B` @0x113234 writes `ag.q:[B` 32 · `ag.r:[B` 32 ·
// `ag.k:[B` 3 · `ag.j:[B` 4 · `ag.i:B` · `ag.j:B` · …, and `ag.a([B)V` reads them back in that order):
//   * `ag.i:B` — the mode it plays now. `ag.y()V` @0x114e60 (a round clear) raises it after NORMAL and HARDCORE and
//     leaves HELL at 2, so mode ≥ 1 IS a NORMAL clear and mode 2 a HARDCORE clear;
//   * `ag.j:B` — the 달성도 its clears added: `ag.y()V` adds `ag.a()B` @0x115008 = done quests × `ag.m[mode]` / 51,
//     `ag.m = [50, 30, 20]` (`ag.<clinit>` @0x1151c0), and clamps at 100; nothing else but a new game (`ag.x()V`, ← 0)
//     writes it. NORMAL and HARDCORE add 80 at most, so a save above 80 has cleared HELL — for certain. A HELL clear
//     with few quests done can stay at 80 or under and is not seen; the next HELL clear pays it as ever.
// Those clears count as played at EASY (the user's rule), so the rewards that ask for no difficulty are owed.

/// The progress a `/g<slot>` records ([`g_progress`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveProgress {
    /// `ag.i:B` — 0 NORMAL · 1 HARDCORE · 2 HELL, the mode the character plays.
    pub mode: u8,
    /// `ag.j:B` — the cumulative 달성도 (0..=100) the round clears added.
    pub achievement: u8,
}

/// Where `ag.i:B` sits in the decrypted third blob — after `ag.q` (32) · `ag.r` (32) · `ag.k` (3) · `ag.j:[B` (4) —
/// and `ag.j:B` right after it.
const G2_MODE_AT: usize = 32 + 32 + 3 + 4;
const G2_ACHIEVEMENT_AT: usize = G2_MODE_AT + 1;

/// `ag.m:[B` — the 달성도 one clear of each mode adds at most (`ag.<clinit>` @0x1151c0: 50 · 30 · 20).
pub const ACHIEVEMENT_WEIGHTS: [u8; 3] = [50, 30, 20];
/// The most a NORMAL clear and a HARDCORE clear add together: a save above it has cleared HELL.
pub const ACHIEVEMENT_BEFORE_HELL: u8 = ACHIEVEMENT_WEIGHTS[0] + ACHIEVEMENT_WEIGHTS[1];

/// The mode and the 달성도 a `/g<slot>` records — `None` for a file that is not a whole save ([`g_complete`]) or
/// values the game cannot write (a mode above HELL, a 달성도 above its clamp of 100).
pub fn g_progress(g: &[u8]) -> Option<SaveProgress> {
    if !g_complete(g) {
        return None;
    }
    // `g_complete` walked the five blobs already: these reads are inside the file.
    let mut p = 0usize;
    for _ in 0..2 {
        p += 2 + usize::from(u16::from_be_bytes([g[p], g[p + 1]]));
    }
    let n = usize::from(u16::from_be_bytes([g[p], g[p + 1]]));
    // The blob's length counts its checksum byte.
    if n < G2_ACHIEVEMENT_AT + 1 + 1 {
        return None;
    }
    let key = G_KEYS[2];
    let plain = |i: usize| g[p + 2 + i] ^ key[(i + 1) % key.len()];
    let progress = SaveProgress { mode: plain(G2_MODE_AT), achievement: plain(G2_ACHIEVEMENT_AT) };
    (progress.mode <= MODE_GUARDIAN && progress.achievement <= 100).then_some(progress)
}

/// ★ 2026-10-08 가디언: the fourth round — `ag.i` 3, reached only by a HELL clear under this build (the game's own
/// `ag.y()V` with `guest_anchors::GuestBuild::guardian_mode_open`). Its 달성도 weight is 0 (`ag.m[3]`, the byte would
/// wrap past 100), so a guardian clear is never seen in these two bytes — only its receipt says it happened.
pub const MODE_GUARDIAN: u8 = 3;

/// The modes a save proves cleared — NORMAL when it plays HARDCORE or above, HARDCORE when it plays HELL or above,
/// HELL when it plays GUARDIAN (only a HELL clear gets there) or when its 달성도 is above [`ACHIEVEMENT_BEFORE_HELL`]
/// — in mode order. A guardian clear leaves no mark here (its weight is 0).
pub fn cleared_before(p: SaveProgress) -> Vec<u8> {
    let mut out = Vec::new();
    if p.mode >= 1 {
        out.push(0);
    }
    if p.mode >= 2 {
        out.push(1);
        if p.mode >= MODE_GUARDIAN || p.achievement > ACHIEVEMENT_BEFORE_HELL {
            out.push(2);
        }
    }
    out
}

/// What a slot's character gets when it is LOADED (이어하기).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loaded {
    /// The committed record is this save's.
    Committed(AvatarState),
    /// The save happened after the pending record was written and its commit did not (the
    /// crash window): the pending state — to be committed now with `bind`.
    Recovered { state: AvatarState, bind: Bind },
    /// No record is this save's: an empty character. The reason is for the log.
    Legacy(&'static str),
}

/// [`Loaded`] from the slot's `/g` on disk and its two records (module header).
pub fn resolve_load(g: Option<&[u8]>, committed: Option<&Record>, pending: Option<&Record>) -> Loaded {
    let Some(g) = g else {
        return Loaded::Legacy("the slot has no /g on disk");
    };
    let now = fnv1a64(g);
    // ⛔ No let-chains — edition 2021 (the selftest and the probe `include!` this file).
    let committed = committed.filter(|c| c.kind == RecordKind::Committed);
    if let Some(c) = committed.filter(|c| c.key == now) {
        return Loaded::Committed(c.state.clone());
    }
    if let Some(p) = pending.filter(|p| p.kind == RecordKind::Pending) {
        let from_this_chain = committed.is_none_or(|c| c.key == p.key);
        if from_this_chain && p.key != now && g_complete(g) {
            return Loaded::Recovered { state: p.state.clone(), bind: now };
        }
    }
    if committed.is_some() {
        Loaded::Legacy("the record belongs to another save")
    } else {
        Loaded::Legacy("no record (a save from before costumes, or never committed)")
    }
}

/// The slot's `/g` was just rewritten by a save: what to commit — `None` while the file is
/// not whole (look again next poll) or when there is nothing to commit. The pending record is
/// the state AS OF the save's first edge — exactly what the game's save captured; the
/// worker's copy is the fallback (no journal — the vault had no root yet).
pub fn commit_after_save(g: &[u8], pending: Option<&Record>, working: Option<&AvatarState>) -> Option<(Bind, AvatarState)> {
    if !g_complete(g) {
        return None;
    }
    let bind = fnv1a64(g);
    let state = match pending {
        Some(p) if p.kind == RecordKind::Pending && p.key != bind => p.state.clone(),
        _ => working?.clone(),
    };
    Some((bind, state))
}

/// A character id from any seed (the caller mixes the clock in) — splitmix64, never 0.
pub fn character_id(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    if z == 0 {
        1
    } else {
        z
    }
}

/// The parts a costume may draw, in the order of `guest_anchors::AvatarPatch::PARTS` (this
/// file has no dependencies, so the list is repeated; `hero2java`'s tests hold the two
/// equal): 1 body · 2 arms · 3 legs · ★ A3b 0 head · 6 weapon · 7 off-hand.
pub const COSTUME_PARTS: [u8; 6] = [1, 2, 3, 0, 6, 7];

/// ★ 2026-10-01 (율리케의 웨딩드레스): a GOWN — the armour item of a set that also carries a
/// SKIRT, a legs sheet under the set's number plus this (`hero/3/<n + 1000>`; the packaging tool's
/// part `skirt`, files `skirt_0..10.png`) — draws the legs too, over whatever the shoes slot
/// wears. A dress's skirt is the dress's (the shoes slot's legs would be trousers under it), and
/// a floor-length gown hides the shoes; without the dress the shoes item draws its legs as ever.
pub const SKIRT_SHEET_OFFSET: i32 = 1000;
/// ★ 2026-10-09 (사용자: *"드레스류 아이템 입었을떄 신발아바타를 바꿔줘 아무런 변경이없은데 … 앞이나 뒤를 덮지 않는
/// 아바타라면 신발아바타를바꿨을떄 변경이 되어야"*): a gown that leaves the legs in sight (207's coat tails, 210's slit
/// skirt) carries its skirt as two LAYERS (`skirtunder_*` behind the legs · `skirtover_*` in front), and the asset
/// builds, for every other set with legs, `hero/3/<dress × this + shoes>` = the layers around THOSE shoes' legs
/// (`costume_sheets::skirt_combo_members`) — so the shoes slot shows under the dress. Without such a sheet (a gown that
/// covers the legs front and back — 201 · 209 —, the gown's own shoes, an old asset) the skirt sheet stays. The game
/// takes the six-digit number as it is (probe: `--h2-avatar 700:…,leg:210208` reloads and draws it).
pub const SKIRT_OVER_SHOES: i32 = 1000;
/// The legs sheet of gown `dress` over the legs of set `shoes` ([`SKIRT_OVER_SHOES`]).
pub fn skirt_over_shoes(dress: u16, shoes: u16) -> i32 {
    i32::from(dress) * SKIRT_OVER_SHOES + i32::from(shoes)
}
/// The legs' place in [`COSTUME_PARTS`].
const LEGS_AT: usize = 2;
const _: () = assert!(COSTUME_PARTS[LEGS_AT] == 3);

/// The parts loader's override for what the slots wear (`worn` = the set each slot wears,
/// `SLOT_*` order): each part ([`COSTUME_PARTS`]) takes the sheet of the set its slot
/// ([`SLOT_OF_PART`]) wears when the jar carries it (`has`, which knows the character's class),
/// and passes through otherwise — a slot wearing nothing, or a set that does not draw that part,
/// leaves it to the equipment. Two slots may wear two different sets. ★ The one exception: an
/// armour item with a skirt ([`SKIRT_SHEET_OFFSET`]) takes the legs from the shoes slot — ★ 2026-10-09
/// unless the jar carries that gown over the shoes slot's set ([`skirt_over_shoes`]): then those legs
/// under the gown.
pub fn costume_parts(worn: [Option<u16>; SLOTS], has: impl Fn(u8, i32) -> bool) -> [(i32, i32); 6] {
    let mut out = [(-1, -1); 6];
    for (i, part) in COSTUME_PARTS.iter().enumerate() {
        if let Some(n) = worn[usize::from(SLOT_OF_PART[i])] {
            if has(*part, i32::from(n)) {
                out[i] = (i32::from(n), -1);
            }
        }
    }
    if let Some(n) = worn[usize::from(SLOT_ARMOUR)] {
        let skirt = i32::from(n) + SKIRT_SHEET_OFFSET;
        if has(COSTUME_PARTS[LEGS_AT], skirt) {
            out[LEGS_AT] = (skirt, -1);
            if let Some(s) = worn[usize::from(SLOT_SHOES)].filter(|s| *s != n) {
                let over = skirt_over_shoes(n, s);
                if has(COSTUME_PARTS[LEGS_AT], over) {
                    out[LEGS_AT] = (over, -1);
                }
            }
        }
    }
    out
}

/// The journal, written at the FIRST `/g` edge of a save (module header): the state being
/// saved, keyed by the hash of the `/g` it replaces (`current_g`, still the old save at that
/// edge; `None` = the slot had none). Returns the base it used.
pub fn write_journal(files: &AvatarFiles, slot: u8, current_g: Option<&[u8]>, working: &AvatarState) -> std::io::Result<Bind> {
    let base = current_g.map_or(UNBOUND, fnv1a64);
    files.write_pending(slot, base, working)?;
    Ok(base)
}

/// ★ A3c: what [`grant_into_record`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Granted {
    /// The `(set, slot)` items added (the character already owned the rest).
    pub added: Vec<(u16, u8)>,
    /// Items the character owns now.
    pub owned: usize,
    /// How the slot's record resolved before the grant (for the log).
    pub was: &'static str,
}

/// ★ A3c: the one-shot grant — the app's boot marker (`vault::costume_grant_request`) and the
/// probe's `--h2-costume-marker`: every item of `items` the character of save slot `slot` does
/// not own yet, written straight into the slot's COMMITTED record bound to the `/g` on disk, so
/// the next 이어하기 finds them with no save needed. Ownership only — nothing is worn (the user's
/// rule: a character looks only like what the wardrobe put on it). The character is the one the
/// slot resolves to ([`resolve_load`]; a journal the crash left is taken as the recovery would
/// take it); a save without a record gets a fresh character, the way 이어하기 would. `Err` =
/// nothing written: no save in the slot, or the write failed. Run only while no game holds the
/// slot's character (before the emulator runs) — the game would save over it otherwise.
pub fn grant_into_record(files: &AvatarFiles, slot: u8, g: Option<&[u8]>, items: &[(u16, u8)], seed: u64) -> Result<Granted, String> {
    let Some(g) = g else {
        return Err(format!("slot {slot} has no save (/g{slot}) — nothing granted"));
    };
    let mut errors = Vec::new();
    let (committed, pending) = files.read(slot, &mut errors);
    if let Some(e) = errors.first() {
        return Err(format!("slot {slot}: unreadable record {e} — nothing granted"));
    }
    let (mut state, was) = match resolve_load(Some(g), committed.as_ref(), pending.as_ref()) {
        Loaded::Committed(s) => (s, "its record"),
        Loaded::Recovered { state, .. } => (state, "its journal (recovered)"),
        Loaded::Legacy(_) => (AvatarState::fresh(character_id(seed ^ (u64::from(slot) << 56))), "no record — a new costume record"),
    };
    let added = state.grant_missing(items, 0);
    files.write_committed(slot, fnv1a64(g), &state).map_err(|e| format!("slot {slot}: the record could not be written: {e}"))?;
    files.remove_pending(slot);
    Ok(Granted { added, owned: state.items.len(), was })
}

/// ★ A5 (2026-10-01): the costume set numbers a grant marker may name — `costumes/<n>_<name>/`,
/// 200..=999 (the packaging tool's range, `costume_sheets.py` COSTUME_MIN/MAX). A marker that names
/// sets grants only those sets' items (user: 「모든 세이브슬롯에 각각 새로운 아바타들 전부」 — the new
/// sets, not every reward set); one that names none grants every item the asset makes, as before.
pub const MARKER_SET_MIN: u32 = 200;
pub const MARKER_SET_MAX: u32 = 999;

/// A grant marker's numbers — its bytes as written by anything: UTF-8 with or without a BOM,
/// UTF-16 LE/BE with a BOM (PowerShell 5.1's `>` writes UTF-16LE), CRLF. Every run of ASCII
/// digits is a number.
fn grant_marker_numbers(bytes: &[u8]) -> Vec<u32> {
    let text: String = match bytes {
        [0xff, 0xfe, rest @ ..] => char::decode_utf16(rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]))).map(|r| r.unwrap_or('\u{fffd}')).collect(),
        [0xfe, 0xff, rest @ ..] => char::decode_utf16(rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]]))).map(|r| r.unwrap_or('\u{fffd}')).collect(),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    text.split(|c: char| !c.is_ascii_digit()).filter_map(|t| t.parse::<u32>().ok()).collect()
}

/// ★ A3c: the save slots a grant marker names ([`grant_marker_numbers`]): the slots `1..=slot_max`,
/// ascending, once each.
pub fn grant_marker_slots(bytes: &[u8], slot_max: u8) -> Vec<u8> {
    let mut slots: Vec<u8> = grant_marker_numbers(bytes).into_iter().filter(|n| (1..=u32::from(slot_max)).contains(n)).map(|n| n as u8).collect();
    slots.sort_unstable();
    slots.dedup();
    slots
}

/// ★ A5: the costume sets a grant marker names ([`MARKER_SET_MIN`]..=[`MARKER_SET_MAX`]), ascending,
/// once each — empty = every set.
pub fn grant_marker_sets(bytes: &[u8]) -> Vec<u16> {
    let mut sets: Vec<u16> = grant_marker_numbers(bytes).into_iter().filter(|n| (MARKER_SET_MIN..=MARKER_SET_MAX).contains(n)).map(|n| n as u16).collect();
    sets.sort_unstable();
    sets.dedup();
    sets
}

/// ★ A3c: a grant marker's whole run after it was found and consumed — the app's
/// (`vault::grant_costumes_on_request`) and the probe's (`--h2-costume-marker`) one code: the
/// slots it names ([`grant_marker_slots`]), then [`grant_into_record`] for each with the slot's
/// `/g` as `live_g` reads it. ★ A4: `items_for(class)` = the items the asset makes for a
/// character of that class (the slot's own, read from its save — [`g_class`]; `None` = any
/// class): a berserker gets no off-hand item, a knight the shield, a rogue the second dagger.
/// ★ A5: a marker that also names costume sets ([`grant_marker_sets`] — `1 2 3 4 5 202 203 204`)
/// grants only those sets' items. Nothing is written when it names no slot or the asset makes no
/// item (of those sets). Returns the lines for the log.
pub fn grant_marker_run(files: &AvatarFiles, marker: &[u8], slot_max: u8, items_for: &dyn Fn(Option<u8>) -> Vec<(u16, u8)>, live_g: &dyn Fn(u8) -> Option<Vec<u8>>, seed: u64) -> Vec<String> {
    let slots = grant_marker_slots(marker, slot_max);
    if slots.is_empty() {
        return vec![format!("costume grant: the marker names no save slot 1..{slot_max} ({:?}) — nothing done", String::from_utf8_lossy(marker))];
    }
    let sets = grant_marker_sets(marker);
    let items_of = |class: Option<u8>| -> Vec<(u16, u8)> {
        let mut items = items_for(class);
        if !sets.is_empty() {
            items.retain(|(set, _)| sets.contains(set));
        }
        items
    };
    if items_of(None).is_empty() {
        return vec![if sets.is_empty() {
            "costume grant: the asset makes no avatar item (no catalog, or no sheets) — nothing done".to_string()
        } else {
            format!("costume grant: the asset makes no item of the named set(s) {sets:?} — nothing done")
        }];
    }
    let only = if sets.is_empty() { String::new() } else { format!(" of set(s) {sets:?}") };
    slots
        .into_iter()
        .map(|slot| {
            let g = live_g(slot);
            let class = g.as_deref().and_then(g_class);
            match grant_into_record(files, slot, g.as_deref(), &items_of(class), seed) {
                Ok(r) => format!("costume grant: slot {slot} ({}) (from {}) gained {} item(s){only} {:?} — owns {} now, nothing worn", class_label(class), r.was, r.added.len(), r.added, r.owned),
                Err(e) => format!("costume grant: {e}"),
            }
        })
        .collect()
}

// ── ★ A4: the round-clear rewards ────────────────────────────────────────────

/// The receipt source of a round-clear reward ([`Receipt::source`]) — with the mode, the key
/// that pays each character once per mode cleared. A4's two rewards and A5's 수영장 파티 use it
/// (one reward per mode under it); a second reward of the same mode needs a source of its own.
pub const CLEAR_REWARD_SOURCE: u32 = 1;
/// ★ A5: the receipt source of HELL's second reward, 할로윈 — HELL already pays 수영장 파티 under
/// [`CLEAR_REWARD_SOURCE`]. A receipt names its SET this way, whatever difficulty the set asks for
/// (★ 2026-10-02 the two traded difficulties; their receipts stayed with the sets).
pub const CLEAR_REWARD_SOURCE_HALLOWEEN: u32 = 2;
/// ★ 2026-10-06: the receipt source of HELL's third reward, 할로윈 파티 205 (HARD or above).
pub const CLEAR_REWARD_SOURCE_HALLOWEEN_PARTY: u32 = 3;
/// ★ 2026-10-08 가디언 (user, Q9: *"가디언 이지 클리어 시 제복 아바타 획득"*): the receipt source of 제복 206 — a GUARDIAN clear at
/// EASY or above. Never granted for a past clear (the mode bytes cannot show one, `MODE_GUARDIAN`).
pub const CLEAR_REWARD_SOURCE_UNIFORM: u32 = 4;
/// ★ 2026-10-09 (user — four new sets, one picture each: *"첫번쨰 스크린샷은 가디언 모드 이지 클리어했을떄, 두번쨰 스크린샷은 가디언
/// 노말 클리어했을떄, 세번째는 가디언 하드 클리어했을떄야. 추가로 4번쨰는 헬모드 하드 클리어했을떄야"*): the receipt sources of
/// 진홍의 검사 207 (GUARDIAN, EASY or above) · 순백의 해머 208 (GUARDIAN, NORMAL or above) · 달빛 엘프 209 (GUARDIAN, HARD or above) ·
/// 백은의 사신 210 (HELL, HARD or above) — one each, so no reward of a mode reads another's receipt as its own.
pub const CLEAR_REWARD_SOURCE_GUARDIAN_EASY: u32 = 5;
pub const CLEAR_REWARD_SOURCE_GUARDIAN_NORMAL: u32 = 6;
pub const CLEAR_REWARD_SOURCE_GUARDIAN_HARD: u32 = 7;
pub const CLEAR_REWARD_SOURCE_HELL_HARD: u32 = 8;

/// ★ 2026-10-06 「심안」 (user: *"lv1 기본 패시브기때문에 독연마의 이미 찍은 포인트 전부를 돌려줘야해"*): the receipt (mode 0,
/// occurrence 0) that the rogue's one-time refund of what 독 연마 held before this build was paid
/// (`hero2java::h2_born_skill_repair`). The level byte cannot tell a 1 bought before the build from the 1 handed
/// out — this record can, and it rides the save like the costumes: the refund is in the character the game
/// saves, the receipt in the record committed with that save, and a reload that did not save takes both back.
pub const RECEIPT_SIGHT_REFUND: u32 = 100;
/// ★ 2026-10-07 (2) 버서커 몸통치기 retired: its points refunded once (`hero2java::h2_bodyslam_retire`) — the receipt's
/// `source`, the same way as 심안's (kept only with the save that carries the refund).
pub const RECEIPT_BODYSLAM_REFUND: u32 = 101;

/// One round-clear reward: clearing `mode` (`ag.i:B` as it was when `ag.y()V` ran — 0 NORMAL ·
/// 1 HARDCORE · 2 HELL) gives set `set`'s items for the character's class — ★ A5 when the round
/// was played at 난이도 `min_difficulty` or above (0 EASY = any: the lowest setting; above it,
/// [`DIFFICULTY_SHARE_PCT`] of the round's done quests must have been done there) — paid once per
/// character under the receipt `(source, mode)`, and the host raises the game's own notice with
/// `/t/itmtp` row `notice_row` (appended by `tools_re/make_hero2_asset.py`, ITMTP_APPEND).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClearReward {
    pub mode: u8,
    pub set: u16,
    pub min_difficulty: u8,
    pub source: u32,
    pub notice_row: u32,
}

/// ★ A4 (2026-10-01, user: *"노말보상으로 이안시리즈 아이템을 4개 or 5개 … 하드코어 클리어 보상으로는
/// 율리케 시리즈 아바타 3개"*): NORMAL → 이안 200 (「이안 아바타 획득」) · HARDCORE → 율리케 201
/// (「율리케 아바타 획득」). ★ A5 (same day, user: *"… 노말클리어시 이안 아바타도 노말 -이지 난이도 이상 클리어하면
/// 주는거고, 하드 클리어도 하드 이지난이도 이상"*): the first two at EASY or above (any 난이도). ★ HELL (2026-10-02,
/// user — the two A5 HELL sets traded places: *"이미 헬모드로 클리어를 한 사용자들은 헬모드 이지모드 클리어한 조건으로
/// 간주하고 클레르, 이안,율리케, 할로윈 해골 아바타를 전부 가지고있어야해. 그리고 헬모드 노말 이상 클리어하면 이제 클레르
/// 비키니를 받을 수있는거야"*, confirmed when asked): 할로윈 204 at EASY or above (「할로윈 아바타 획득」) and 수영장 파티
/// 203 at NORMAL or above (「수영장 아바타 획득」) — a HELL clear at NORMAL pays both. In the order paid (and noticed).
/// ★ 2026-10-06 (user: 「할로윈 파티」 오딘 가면 · 송곳니 · 손톱 · 미니 오딘주먹 → asked how it is earned: *"hell 난이도 hard이상
/// 클리어"*): 할로윈 파티 205 at HARD or above (「오딘 아바타 획득」, `/t/itmtp` 69) — a HELL clear at HARD pays all three.
/// Like 수영장 파티 it is not granted for a past clear (`AvatarBook::grant_earned` pays EASY rewards only — ★ 2026-10-09 save one
/// case: a record holding a GUARDIAN receipt is owed every HELL set, [`implied_clear`]).
/// ★ 2026-10-09 (user, four pictures): + 백은의 사신 210 for HELL at HARD or above (「사신 아바타 획득」, 74) — a HELL clear at
/// HARD pays all four HELL sets — and GUARDIAN's 진홍의 검사 207 at EASY or above (「진홍 아바타 획득」, 71) beside 제복 206,
/// 순백의 해머 208 at NORMAL or above (「해머 아바타 획득」, 72), 달빛 엘프 209 at HARD or above (「엘프 아바타 획득」, 73). And a
/// GUARDIAN clear — any 난이도 — counts as a HELL clear at HARD ([`implied_clear`]): every HELL set comes with it.
pub const CLEAR_REWARDS: [ClearReward; 10] = [
    ClearReward { mode: 0, set: 200, min_difficulty: 0, source: CLEAR_REWARD_SOURCE, notice_row: 65 },
    ClearReward { mode: 1, set: 201, min_difficulty: 0, source: CLEAR_REWARD_SOURCE, notice_row: 66 },
    ClearReward { mode: 2, set: 204, min_difficulty: 0, source: CLEAR_REWARD_SOURCE_HALLOWEEN, notice_row: 68 },
    ClearReward { mode: 2, set: 203, min_difficulty: 1, source: CLEAR_REWARD_SOURCE, notice_row: 67 },
    ClearReward { mode: 2, set: 205, min_difficulty: 2, source: CLEAR_REWARD_SOURCE_HALLOWEEN_PARTY, notice_row: 69 },
    ClearReward { mode: 2, set: 210, min_difficulty: 2, source: CLEAR_REWARD_SOURCE_HELL_HARD, notice_row: 74 },
    // ★ 2026-10-08: GUARDIAN (mode 3, `MODE_GUARDIAN`) → 제복 206 at EASY or above (「제복 아바타 획득」, `/t/itmtp` 70).
    ClearReward { mode: MODE_GUARDIAN, set: 206, min_difficulty: 0, source: CLEAR_REWARD_SOURCE_UNIFORM, notice_row: 70 },
    ClearReward { mode: MODE_GUARDIAN, set: 207, min_difficulty: 0, source: CLEAR_REWARD_SOURCE_GUARDIAN_EASY, notice_row: 71 },
    ClearReward { mode: MODE_GUARDIAN, set: 208, min_difficulty: 1, source: CLEAR_REWARD_SOURCE_GUARDIAN_NORMAL, notice_row: 72 },
    ClearReward { mode: MODE_GUARDIAN, set: 209, min_difficulty: 2, source: CLEAR_REWARD_SOURCE_GUARDIAN_HARD, notice_row: 73 },
];

/// HELL — `ag.i:B` 2.
pub const MODE_HELL: u8 = 2;

/// ★ 2026-10-09 (user: *"가디언 이지클리어하면 헬모드 하드아바타 해금조건까지 클리어한거나 마찬가지인거야"*): the mode whose
/// rewards a clear of `mode` pays as well, every one of them whatever the difficulty it asks for — a GUARDIAN clear (any
/// 난이도, EASY is the least) is HELL cleared at HARD. They are paid under HELL's own receipts, so neither path pays one
/// twice. Only the round below: a HELL clear already proves HARDCORE and NORMAL, and their rewards ask for no difficulty
/// (the save shows them, [`cleared_before`]).
pub fn implied_clear(mode: u8) -> Option<u8> {
    (mode == MODE_GUARDIAN).then_some(MODE_HELL)
}

/// ★ A5: the share of a round's done quests that must have been done at a reward's difficulty or
/// above (user: *"진행도가 95프로 이상"*).
pub const DIFFICULTY_SHARE_PCT: u32 = 95;

/// The rewards for clearing `mode`, in the order paid.
pub fn clear_rewards(mode: u8) -> Vec<ClearReward> {
    CLEAR_REWARDS.iter().copied().filter(|r| r.mode == mode).collect()
}

/// ★ 2026-10-16 「보스 레이드」: should the temple load WITH the raid gate for the character whose `/g` bytes `g` the game is
/// reading — the committed record of file slot `slot`, bound to exactly that save, holds a GUARDIAN HARD receipt
/// ([`AvatarState::cleared_guardian_hard`]). Decided on the `/g` read itself (the map that follows it in the same load
/// must already know, before any book is open): a record that is not there, does not read, or is bound to another save
/// says no — the shipped 신전. The book in the world and the clear hook keep the same answer current afterwards.
pub fn raid_gate_for_save(files: &AvatarFiles, slot: u8, g: &[u8]) -> bool {
    let mut errors = Vec::new();
    let (committed, _pending) = files.read(slot, &mut errors);
    committed.is_some_and(|r| r.key == fnv1a64(g) && r.state.cleared_guardian_hard())
}

/// ★ A5: does a round with `(done at the difficulty or above, done)` = `progress` qualify for a
/// reward that asks for `min_difficulty`? EASY asks nothing; above it, at least
/// [`DIFFICULTY_SHARE_PCT`] of at least one done quest (a round the host saw nothing of proves nothing).
pub fn difficulty_met(min_difficulty: u8, progress: (u32, u32)) -> bool {
    let (at, done) = progress;
    min_difficulty == 0 || (done > 0 && at * 100 >= DIFFICULTY_SHARE_PCT * done)
}

/// ★ A5 (2026-10-01, user: *"아무 조건없이 맨처음부터 가지고있는(새게임 시작 시 부터 가지게 될)아바타로 클레르 머리,
/// 클레르의 잠옷 상의, 클레르의 잠옷 바지"*): the sets every character owns from the start — 클레르의 잠옷 202. No
/// condition and no receipt: granted whenever a character enters the world ([`AvatarBook::grant_starter`]) —
/// a new game, and any load (a save from before this build too), so owning them is the rule, not an event.
pub const STARTER_SETS: [u16; 1] = [202];

/// The game's word for a mode (`t/com.tdf` 10..12), for the log.
pub fn mode_label(mode: u8) -> &'static str {
    match mode {
        0 => "NORMAL",
        1 => "HARDCORE",
        2 => "HELL",
        3 => "GUARDIAN",
        _ => "?",
    }
}

/// What [`AvatarBook::clear_rewards`] did — one per reward of the mode cleared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClearOutcome {
    /// The set's items for the character's class were added (none worn) — kept only if the
    /// game then saves, like the clear itself.
    Paid { reward: ClearReward, ids: Vec<u32>, slots: Vec<u8> },
    /// The mode cleared has no reward.
    NoReward,
    /// This character was paid that reward already (its receipt).
    AlreadyPaid { reward: ClearReward },
    /// The asset makes none of the set's items for this character's class — nothing paid and
    /// no receipt written (an asset built without the set).
    NothingToGive { reward: ClearReward },
    /// ★ A5: the round was not played at the reward's difficulty — `done_at` of its `done` quests
    /// were done at it or above, short of [`DIFFICULTY_SHARE_PCT`]. No receipt: a later clear of
    /// that mode played at it pays it.
    Short { reward: ClearReward, done_at: u32, done: u32 },
    /// Nobody in the world.
    NoCharacter,
}

// ── the book: the worker's side, shared by the app and the probe ─────────────

/// What the book needs from its host — the app's disk and vault (`emulator::AppAvatarIo`), or
/// the probe's in-memory filesystem. Slots are FILE slots, 1..=5 (`/g1` .. `/g5`).
pub trait AvatarIo {
    /// The slot's `/g` as it is now.
    fn live_g(&self, slot: u8) -> Option<Vec<u8>>;
    /// Where the records live (`None` before the app has a root).
    fn files(&self) -> Option<AvatarFiles>;
    /// How many `/g<slot>` edges the guest has produced.
    fn g_edges(&self, slot: u8) -> u64;
    /// The character in the world changed — for the journal written inside the next save.
    fn publish(&self, working: Option<&AvatarState>);
    /// A commit landed: the next save of `slot` journals again.
    fn rearm(&self, slot: u8);
    /// A line for the log (`warn` = something the reader should look at).
    fn note(&self, warn: bool, line: String);
}

/// ★★ The character in the world and its costumes, over the life of a run:
/// * [`AvatarBook::enter_world`] — the world was entered from the shell: 새로하기 → a fresh
///   character (the slot's old record stays until this one first saves — the game warns the
///   old save is deleted only "플레이 중 저장하면"), 이어하기 → [`resolve_load`] of the slot;
/// * [`AvatarBook::poll`] — a slot's `/g` was rewritten: commit once it reads whole
///   ([`commit_after_save`]);
/// * [`AvatarBook::leave_world`] — back to the title: nothing is carried to the next load
///   (that load resolves from the disk, which is what makes a death roll back).
pub struct AvatarBook {
    /// `(file slot, state)` of the character in the world.
    pub working: Option<(u8, AvatarState)>,
    /// Slots whose `/g` was rewritten and not yet committed: `(slot, polls waited)`.
    waiting: Vec<(u8, u16)>,
    g_seen: [u64; 6],
    issued: u64,
    /// ★ A5: the next quest look is the first after a load ([`AvatarState::observe_quests`]).
    quest_baseline: bool,
    /// ★★ 2026-10-22 가디언 레벨 (review P1): the GL the last save was MADE at, as the guest's `/g` writer snapshotted
    /// it ([`AvatarBook::note_save_gl`]) — the next commit takes it over the state it commits (the journal holds the last
    /// PUBLISHED GL, which a GL-up in the save's own tick has not reached yet).
    save_gl: Option<u16>,
    /// ★ 2026-10-28 (code review P1): slots (bit `1 << slot`) whose last save's commit did NOT land — the record on the
    /// disk is the save before's and the journal holds the last PUBLISHED state (not the save's GL), so the disk is not one
    /// moment until a commit of that slot lands (its next save, or a load that recovers it). [`Self::caught_up`] answers
    /// `None` while one is set; leaving the world keeps it (the disk did not change).
    failed: u8,
}

impl AvatarBook {
    /// Polls a rewritten `/g` may stay incomplete before the commit is given up (a save that
    /// died half way never becomes whole).
    pub const COMMIT_POLLS: u16 = 100;

    pub fn new(io: &dyn AvatarIo) -> Self {
        let mut g_seen = [0u64; 6];
        for (slot, seen) in g_seen.iter_mut().enumerate() {
            *seen = io.g_edges(slot as u8);
        }
        Self { working: None, waiting: Vec::new(), g_seen, issued: 0, quest_baseline: false, save_gl: None, failed: 0 }
    }

    /// ★ 2026-10-16 보스 레이드: the character in the world holds a GUARDIAN HARD receipt ([`AvatarState::cleared_guardian_hard`]);
    /// false when nobody is in the world.
    pub fn cleared_guardian_hard(&self) -> bool {
        self.working.as_ref().is_some_and(|(_, s)| s.cleared_guardian_hard())
    }

    /// ★ A5: give the character in the world the starter items it does not own yet (`items` = the
    /// [`STARTER_SETS`] items the asset makes for its class) — none worn. Called on every entry to the
    /// world: a new game owns them from its first frame, and a load gets back any it lacks (a save from
    /// before this build, a death that rolled back past the entry) — they are owned, never earned, so
    /// nothing is kept in a receipt. Returns how many were added.
    pub fn grant_starter(&mut self, io: &dyn AvatarIo, items: &[(u16, u8)]) -> usize {
        let Some((file_slot, s)) = self.working.as_mut() else { return 0 };
        let added = s.grant_missing(items, 0);
        if added.is_empty() {
            return 0;
        }
        io.note(false, format!("avatar: slot {file_slot} owns the starter items {added:?} — none worn"));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        added.len()
    }

    /// ★ 2026-10-02: the character just entered the world from save `progress` ([`g_progress`] of the
    /// slot's `/g`): pay what the save proves was cleared ([`cleared_before`]) and the character holds no
    /// receipt for — each such mode's rewards that ask for no difficulty (a clear from before counts as a
    /// clear at EASY): the items of the set for its class (`slots_for`) it does not own yet, none worn, and
    /// the receipt, so the round-clear path never pays the same reward twice. A set the character owns
    /// already (an adb grant) gets only its receipt.
    ///
    /// Unlike a reward earned in play this is not "kept only if saved": it is a fact of the save on disk, so
    /// the record is committed against that save at once ([`fnv1a64`] of the `/g`, the way the boot grant
    /// writes it) — a reload of the same save finds it paid and says nothing again. Returns the rewards that
    /// added items, in the order paid (one game notice each).
    ///
    /// ★ 2026-10-09: a GUARDIAN clear leaves no mark in the save's bytes, but the record bound to that save holds its
    /// receipt ([`AvatarState::cleared_guardian`]) — a clear at EASY or above. It is owed GUARDIAN's rewards that ask for
    /// no difficulty (진홍의 검사 207 for a character paid 제복 206 by an earlier build) and every HELL reward, whatever
    /// difficulty it asks for ([`implied_clear`]: HELL at HARD).
    pub fn grant_earned(&mut self, io: &dyn AvatarIo, progress: SaveProgress, slots_for: &dyn Fn(u16) -> Vec<u8>) -> Vec<ClearReward> {
        let Some((file_slot, s)) = self.working.as_mut() else { return Vec::new() };
        let file_slot = *file_slot;
        let mut owed: Vec<(ClearReward, &'static str)> = Vec::new();
        for mode in cleared_before(progress) {
            owed.extend(clear_rewards(mode).into_iter().filter(|r| r.min_difficulty == 0).map(|r| (r, "an EASY clear")));
        }
        if s.cleared_guardian() {
            owed.extend(clear_rewards(MODE_GUARDIAN).into_iter().filter(|r| r.min_difficulty == 0).map(|r| (r, "an EASY clear (its guardian receipt)")));
            if let Some(lower) = implied_clear(MODE_GUARDIAN) {
                owed.extend(clear_rewards(lower).into_iter().map(|r| (r, "a HARD clear (a guardian clear counts as one)")));
            }
        }
        let mut noticed = Vec::new();
        let mut changed = false;
        for (reward, why) in owed {
            let mode = reward.mode;
            if s.paid(reward.source, mode, 0) {
                continue;
            }
            let slots = slots_for(reward.set);
            if slots.is_empty() {
                io.note(true, format!("avatar: slot {file_slot}'s save has cleared {} — the asset makes no item of set {} for this class; nothing paid", mode_label(mode), reward.set));
                continue;
            }
            let Some(receipt) = s.receipt(reward.source, mode, 0) else { continue };
            let mut added = Vec::new();
            for slot in slots {
                if !s.owns(reward.set, slot) {
                    added.push(s.grant(reward.set, slot, mode, receipt));
                }
            }
            changed = true;
            io.note(
                false,
                format!(
                    "avatar: slot {file_slot}'s save has cleared {} (mode {}, 달성도 {}) — set {} paid as {why}: {} new instance(s) {added:?} (receipt {receipt}), none worn",
                    mode_label(mode),
                    progress.mode,
                    progress.achievement,
                    reward.set,
                    added.len()
                ),
            );
            if !added.is_empty() {
                noticed.push(reward);
            }
        }
        if changed {
            self.commit_entry(io);
            io.publish(self.working.as_ref().map(|(_, s)| s));
        }
        noticed
    }

    /// The character in the world, committed against its slot's `/g` as it is on disk now — what was
    /// just loaded ([`AvatarBook::grant_earned`]). Nothing when there is no record directory yet or the
    /// file does not read whole; the next save commits it then, as it commits everything.
    fn commit_entry(&self, io: &dyn AvatarIo) {
        let Some((slot, s)) = self.working.as_ref() else { return };
        let (Some(files), Some(g)) = (io.files(), io.live_g(*slot)) else { return };
        if !g_complete(&g) {
            return;
        }
        match files.write_committed(*slot, fnv1a64(&g), s) {
            Ok(()) => io.note(false, format!("avatar: slot {slot} committed with the save it loaded — {} item(s)", s.items.len())),
            Err(e) => io.note(true, format!("avatar: slot {slot} could not be committed with the save it loaded: {e} — the next save commits it")),
        }
    }

    /// ★ A5: the game's quest states (`done[i]` = quest `i` done — `hero2java::h2_quests_done`) as read in
    /// the world, at 난이도 `difficulty`: the character's marks follow them ([`AvatarState::observe_quests`];
    /// the first look after a load is the baseline). Published when a mark changed, so the journal written
    /// inside the next save carries it.
    pub fn observe_quests(&mut self, io: &dyn AvatarIo, done: &[bool; QUESTS], difficulty: u8) {
        let baseline = std::mem::take(&mut self.quest_baseline);
        let Some((file_slot, s)) = self.working.as_mut() else { return };
        let before = s.quests;
        let newly = s.observe_quests(done, difficulty, baseline);
        if s.quests == before {
            return;
        }
        let file_slot = *file_slot;
        let (at, all) = s.run_progress(1);
        let unknown = s.quests.iter().filter(|m| **m == QUEST_UNKNOWN).count();
        if baseline {
            io.note(false, format!("avatar: slot {file_slot} loaded with {all} quest(s) done this round — {unknown} with no mark (done before the host could see: never NORMAL+), {at} at NORMAL+"));
        } else if !newly.is_empty() {
            io.note(false, format!("avatar: slot {file_slot} quest(s) {newly:?} done at {} — this round {at}/{all} at NORMAL+", difficulty_label(difficulty)));
        } else {
            io.note(false, format!("avatar: slot {file_slot} quest marks follow the game ({all} done this round, {at} at NORMAL+)"));
        }
        io.publish(self.working.as_ref().map(|(_, s)| s));
    }

    /// ★ 2026-10-06 「심안」: was the one-time 독 연마 refund paid to the character in the world ([`RECEIPT_SIGHT_REFUND`])
    /// — `None` with nobody in the world (the refund waits for the character's record).
    pub fn sight_refunded(&self) -> Option<bool> {
        self.working.as_ref().map(|(_, s)| s.paid(RECEIPT_SIGHT_REFUND, 0, 0))
    }

    /// ★ 2026-10-06 「심안」: the refund's writes landed (`refund` points back) — its receipt, kept only if saved like every
    /// other change to the state (it is NOT committed now: the save that carries the refunded character carries it).
    /// `false` with nobody in the world or when it was paid already.
    pub fn mark_sight_refunded(&mut self, io: &dyn AvatarIo, refund: i32) -> bool {
        let Some((file_slot, s)) = self.working.as_mut() else { return false };
        let file_slot = *file_slot;
        let Some(id) = s.receipt(RECEIPT_SIGHT_REFUND, 0, 0) else { return false };
        io.note(false, format!("avatar: slot {file_slot} 심안: the 독 연마 refund ({refund} point(s)) recorded (receipt {id}) — kept only if saved"));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        true
    }

    /// ★ 2026-10-07 (2) 몸통치기: was the one-time refund paid to the berserker in the world ([`RECEIPT_BODYSLAM_REFUND`]) —
    /// `None` with nobody in the world.
    pub fn bodyslam_refunded(&self) -> Option<bool> {
        self.working.as_ref().map(|(_, s)| s.paid(RECEIPT_BODYSLAM_REFUND, 0, 0))
    }

    /// ★ 2026-10-07 (2) 몸통치기: the refund's writes landed — its receipt, kept only if saved (as [`Self::mark_sight_refunded`]).
    pub fn mark_bodyslam_refunded(&mut self, io: &dyn AvatarIo, refund: i32) -> bool {
        let Some((file_slot, s)) = self.working.as_mut() else { return false };
        let file_slot = *file_slot;
        let Some(id) = s.receipt(RECEIPT_BODYSLAM_REFUND, 0, 0) else { return false };
        io.note(false, format!("avatar: slot {file_slot} 몸통치기 retired: its refund ({refund} point(s)) recorded (receipt {id}) — kept only if saved"));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        true
    }

    /// The set each avatar slot of the character in the world wears (`SLOT_*` order).
    /// ★★ 2026-10-22 가디언 레벨: the GL on the record of the character in the world (`None` with nobody in it) — what
    /// the level-up's block is armed with (`hero2java::h2_guardian_level_arm_plan`).
    pub fn guardian_level(&self) -> Option<u16> {
        self.working.as_ref().map(|(_, s)| s.gl)
    }

    /// ★★ 2026-10-22 가디언 레벨 (review P1): the guest's `/g` writer took the GL a save is made at — the commit of that
    /// save carries this GL (with the stat points the save holds), whatever the journal was written with.
    pub fn note_save_gl(&mut self, io: &dyn AvatarIo, gl: u16) {
        if self.working.as_ref().is_some_and(|(_, s)| s.gl != gl) {
            io.note(false, format!("avatar: a save made at 가디언 레벨 {gl} (the record said {}) — its commit takes {gl}", self.guardian_level().unwrap_or(0)));
        }
        self.save_gl = Some(gl);
    }

    /// ★★ 2026-10-22 가디언 레벨: the block's GL moved (or the arm plan set it) — the character's record follows, and the
    /// journal written inside the next save carries it. NOT committed now: like every change to the state it is kept
    /// only with a save, together with the stat points the same guest routine gave. `false` with nobody in the world
    /// or nothing to change.
    pub fn set_guardian_level(&mut self, io: &dyn AvatarIo, gl: u16) -> bool {
        let Some((file_slot, s)) = self.working.as_mut() else { return false };
        if s.gl == gl {
            return false;
        }
        let (file_slot, was) = (*file_slot, s.gl);
        s.gl = gl;
        io.note(false, format!("avatar: slot {file_slot} 가디언 레벨 {was} → {gl} — kept only if saved"));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        true
    }

    pub fn worn(&self) -> [Option<u16>; SLOTS] {
        self.working.as_ref().map_or([None; SLOTS], |(_, s)| s.worn_avatars())
    }

    /// The state of the character in the world (the costume window lists it).
    #[allow(dead_code)] // the app's costume window; the selftest builds without it
    pub fn state(&self) -> Option<&AvatarState> {
        self.working.as_ref().map(|(_, s)| s)
    }

    /// ★ A3c: wear owned instance `id` in avatar slot `slot`, or nothing there — the costume
    /// window's OK. `false` (and no change) for an instance not owned or of another slot, or
    /// with nobody in the world. Kept only if saved, like every other change to the state.
    #[allow(dead_code)] // the app's costume window and the probe's; the selftest builds without it
    pub fn wear_instance(&mut self, io: &dyn AvatarIo, slot: u8, id: Option<u32>) -> bool {
        let Some((file_slot, s)) = self.working.as_mut() else { return false };
        if !s.wear(slot, id) {
            return false;
        }
        io.note(false, format!("avatar: slot {file_slot} wears instance {id:?} in avatar slot {} — now {:?} — kept only if saved", slot_key(slot).unwrap_or("?"), s.worn_avatars()));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        true
    }

    /// The world was entered from the shell. `seed` is mixed into a new character's id (the
    /// caller passes the clock).
    pub fn enter_world(&mut self, io: &dyn AvatarIo, slot: u8, new_game: bool, seed: u64) {
        self.issued = self.issued.wrapping_add(1);
        self.save_gl = None;
        // ★ A5: a load's first quest look is the baseline (a new game has no done quest to judge).
        self.quest_baseline = !new_game;
        let fresh = |issued: u64| AvatarState::fresh(character_id(seed ^ (u64::from(slot) << 56) ^ issued.rotate_left(17)));
        let state = if new_game {
            let s = fresh(self.issued);
            io.note(false, format!("avatar: a NEW character in slot {slot} (id {:016x}) owns nothing — the slot's record is replaced by its first save", s.character));
            s
        } else {
            let files = io.files();
            let mut errors = Vec::new();
            let (committed, pending) = match &files {
                Some(f) => f.read(slot, &mut errors),
                None => (None, None),
            };
            for e in errors {
                io.note(true, format!("avatar: unreadable record {e}"));
            }
            let mut had_pending = pending.is_some();
            let g = io.live_g(slot);
            let s = match resolve_load(g.as_deref(), committed.as_ref(), pending.as_ref()) {
                Loaded::Committed(s) => {
                    io.note(false, format!("avatar: slot {slot} loaded — {} item(s), wearing {:?}", s.items.len(), s.worn_avatars()));
                    s
                }
                Loaded::Recovered { state, bind } => {
                    match files.as_ref().map(|f| f.write_committed(slot, bind, &state)) {
                        Some(Ok(())) => {
                            // The disk is one moment again: the record is what this phone itself now plays.
                            self.failed &= !(1u8 << slot);
                            io.note(true, format!("avatar: slot {slot} RECOVERED from its journal — the save landed, its commit did not ({} item(s))", state.items.len()))
                        }
                        Some(Err(e)) => {
                            // ⛔ Keep the journal: it is the only copy of this save's state, and the
                            // next load recovers from it again (a full disk must not become a loss).
                            had_pending = false;
                            io.note(true, format!("avatar: slot {slot} recovered from its journal but the commit failed: {e} — the journal is kept"));
                        }
                        None => {}
                    }
                    state
                }
                Loaded::Legacy(why) => {
                    let s = fresh(self.issued);
                    io.note(committed.is_some(), format!("avatar: slot {slot} loads with no costumes — {why}"));
                    s
                }
            };
            // A journal that was not this load's recovery belonged to a save that never landed.
            if had_pending {
                if let Some(f) = &files {
                    f.remove_pending(slot);
                }
            }
            s
        };
        self.working = Some((slot, state));
        io.publish(self.working.as_ref().map(|(_, s)| s));
    }

    /// Back to the title (or anywhere the character is not): nothing carries over.
    pub fn leave_world(&mut self, io: &dyn AvatarIo) {
        self.save_gl = None;
        if let Some((slot, _)) = self.working.take() {
            io.note(false, format!("avatar: the character of slot {slot} left the world — its costumes are its save's again"));
        }
        io.publish(None);
    }

    /// ★ 2026-10-28 (세이브내보내기, which saves first and then reads the disk): has every `/g` edge
    /// been seen and its commit settled — the record on the disk is the one bound to the `/g` on the
    /// disk? A record left behind would bind to the save before, and the other phone would load the
    /// character with no costumes.
    ///
    /// `Some(false)` = not yet (an edge unseen, a commit waiting — a failure from before is not judged
    /// yet: this save's commit may be the one that lands). `None` = a commit did not land (code review
    /// P1: the write failed and the waiting list emptied all the same, so this answered 「caught up」 and
    /// the export carried the journal's GL 10 for a save made at GL 11).
    pub fn caught_up(&self, io: &dyn AvatarIo) -> Option<bool> {
        if !self.waiting.is_empty() || (1u8..=5).any(|slot| io.g_edges(slot) != self.g_seen[usize::from(slot)]) {
            return Some(false);
        }
        if self.failed != 0 {
            return None;
        }
        Some(true)
    }

    /// Every few frames: slots whose `/g` was rewritten, and their commits.
    pub fn poll(&mut self, io: &dyn AvatarIo) {
        for slot in 1u8..=5 {
            let e = io.g_edges(slot);
            if e != self.g_seen[usize::from(slot)] {
                self.g_seen[usize::from(slot)] = e;
                if !self.waiting.iter().any(|(s, _)| *s == slot) {
                    self.waiting.push((slot, 0));
                }
            }
        }
        if self.waiting.is_empty() {
            return;
        }
        let Some(files) = io.files() else { return };
        let working = self.working.clone();
        let mut still = Vec::new();
        for (slot, polls) in std::mem::take(&mut self.waiting) {
            let Some(g) = io.live_g(slot) else {
                io.note(true, format!("avatar: slot {slot}'s /g was written but cannot be read"));
                continue;
            };
            let mut errors = Vec::new();
            let (_committed, pending) = files.read(slot, &mut errors);
            for e in errors {
                io.note(true, format!("avatar: unreadable record {e}"));
            }
            match commit_after_save(&g, pending.as_ref(), working.as_ref().map(|(_, s)| s)) {
                Some((bind, mut state)) => {
                    // ★★ 2026-10-22 가디언 레벨 (review P1): the GL this save was made at.
                    if let Some(gl) = self.save_gl.take() {
                        state.gl = gl;
                    }
                    match files.write_committed(slot, bind, &state) {
                        Ok(()) => {
                            self.failed &= !(1u8 << slot);
                            io.note(false, format!("avatar: slot {slot} committed with its save (bind {bind:016x}) — {} item(s), wearing {:?}", state.items.len(), state.worn_avatars()));
                            files.remove_pending(slot);
                        }
                        // ⛔ The journal stays: until a commit lands it is what the next load
                        // recovers this save from (its base is the committed record's bind).
                        Err(e) => {
                            self.failed |= 1u8 << slot;
                            io.note(true, format!("avatar: slot {slot} commit FAILED: {e} — its journal is kept for the next load"))
                        }
                    }
                    io.rearm(slot);
                    if let Some((w, _)) = &working {
                        if *w != slot {
                            io.note(true, format!("avatar: the character of slot {w} saved into slot {slot}"));
                        }
                    }
                }
                None if !g_complete(&g) => {
                    if polls + 1 >= Self::COMMIT_POLLS {
                        io.note(true, format!("avatar: slot {slot}'s /g never read whole — the save did not complete; nothing committed"));
                    } else {
                        still.push((slot, polls + 1));
                    }
                }
                None => io.note(false, format!("avatar: slot {slot} saved with no character in the world — nothing to commit")),
            }
        }
        self.waiting = still;
    }

    /// Give the character in the world set `avatar`'s items for `slots` and wear them (test
    /// inputs now, the A4 rewards later). `None` with nobody in the world.
    #[allow(dead_code)] // the probe's `--h2-costume-grant`; A4 in the app
    pub fn grant_and_wear(&mut self, io: &dyn AvatarIo, avatar: u16, slots: &[u8], mode: u8) -> Option<Vec<u32>> {
        let (file_slot, s) = self.working.as_mut()?;
        let ids = s.grant_set(avatar, slots, mode, 0);
        for (&slot, &id) in slots.iter().zip(&ids) {
            s.wear(slot, Some(id));
        }
        io.note(false, format!("avatar: slot {file_slot} gained set {avatar} (instances {ids:?}, slots {slots:?}, mode {mode}) and wears it — kept only if saved"));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        Some(ids)
    }

    /// ★ A4 · ★ A5: the character in the world just cleared `mode` (`ag.y()V` ran — the host's
    /// `ClearHookPatch` counted it): pay each of that mode's rewards ([`clear_rewards`]) once per
    /// character — `slots_for(set)` = the set's items the asset makes for this character's class —
    /// when the round was played at the reward's difficulty ([`difficulty_met`] on the quest marks).
    /// Then the round's marks start over (the game opens every quest for the next round). Nothing is
    /// worn (a character looks only like what the wardrobe put on it), and like the clear itself it is
    /// kept only if the game then saves.
    ///
    /// ★ 2026-10-09: after the mode's own rewards come those of the mode its clear implies ([`implied_clear`] — a
    /// GUARDIAN clear is HELL cleared at HARD): each one not paid yet, whatever difficulty it asks for, under HELL's own
    /// receipt.
    pub fn clear_rewards(&mut self, io: &dyn AvatarIo, mode: u8, slots_for: &dyn Fn(u16) -> Vec<u8>) -> Vec<ClearOutcome> {
        let mut rewards: Vec<(ClearReward, bool)> = clear_rewards(mode).into_iter().map(|r| (r, false)).collect();
        if let Some(lower) = implied_clear(mode) {
            rewards.extend(clear_rewards(lower).into_iter().map(|r| (r, true)));
        }
        let Some((file_slot, s)) = self.working.as_mut() else {
            io.note(true, format!("avatar: {} cleared with nobody in the world — nothing paid", mode_label(mode)));
            return vec![ClearOutcome::NoCharacter];
        };
        let file_slot = *file_slot;
        let mut out = Vec::new();
        if rewards.is_empty() {
            io.note(false, format!("avatar: {} cleared — that mode has no avatar reward", mode_label(mode)));
            out.push(ClearOutcome::NoReward);
        }
        for (reward, implied) in rewards {
            if s.paid(reward.source, reward.mode, 0) {
                let line = if implied {
                    format!("avatar: slot {file_slot} cleared {} — {}'s reward (set {}) was paid already", mode_label(mode), mode_label(reward.mode), reward.set)
                } else {
                    format!("avatar: slot {file_slot} cleared {} again — its reward (set {}) was paid already", mode_label(mode), reward.set)
                };
                io.note(false, line);
                out.push(ClearOutcome::AlreadyPaid { reward });
                continue;
            }
            let (done_at, done) = s.run_progress(reward.min_difficulty);
            if !implied && !difficulty_met(reward.min_difficulty, (done_at, done)) {
                io.note(
                    false,
                    format!(
                        "avatar: slot {file_slot} cleared {} — set {} asks for {} or above on {DIFFICULTY_SHARE_PCT} % of the round: {done_at}/{done} quest(s) — not paid",
                        mode_label(mode),
                        reward.set,
                        difficulty_label(reward.min_difficulty)
                    ),
                );
                out.push(ClearOutcome::Short { reward, done_at, done });
                continue;
            }
            let slots = slots_for(reward.set);
            if slots.is_empty() {
                io.note(true, format!("avatar: slot {file_slot} cleared {} — the asset makes no item of set {} for this class; nothing paid", mode_label(mode), reward.set));
                out.push(ClearOutcome::NothingToGive { reward });
                continue;
            }
            let Some(receipt) = s.receipt(reward.source, reward.mode, 0) else {
                out.push(ClearOutcome::AlreadyPaid { reward }); // not reached: `paid` was asked above
                continue;
            };
            let ids = s.grant_set(reward.set, &slots, mode, receipt);
            let why = if implied {
                format!(" (counts as {} at HARD)", mode_label(reward.mode))
            } else if reward.min_difficulty == 0 {
                String::new()
            } else {
                format!(" ({done_at}/{done} quest(s) at {} or above)", difficulty_label(reward.min_difficulty))
            };
            io.note(
                false,
                format!(
                    "avatar: slot {file_slot} cleared {}{why} — reward set {} paid: instances {ids:?} in slots {slots:?} (receipt {receipt}), none worn — kept only if saved",
                    mode_label(mode),
                    reward.set
                ),
            );
            out.push(ClearOutcome::Paid { reward, ids, slots });
        }
        s.reset_run();
        io.publish(self.working.as_ref().map(|(_, s)| s));
        out
    }

    /// Give the character in the world set `avatar`'s items for `slots` WITHOUT wearing them.
    /// `None` with nobody in the world.
    #[allow(dead_code)] // the probe's `--h2-costume-give`
    pub fn give(&mut self, io: &dyn AvatarIo, avatar: u16, slots: &[u8], mode: u8) -> Option<Vec<u32>> {
        let (file_slot, s) = self.working.as_mut()?;
        let ids = s.grant_set(avatar, slots, mode, 0);
        io.note(false, format!("avatar: slot {file_slot} was given set {avatar} (instances {ids:?}, slots {slots:?}, mode {mode}) — not worn, kept only if saved"));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        Some(ids)
    }

    /// Wear the first owned item of set `avatar` in every slot where one is owned (the other
    /// slots keep what they wear), or — `None` — nothing in any slot. `false` when nothing of the
    /// set is owned, or nobody is in the world.
    #[allow(dead_code)] // the probe's `--h2-costume-wear`
    pub fn wear_avatar(&mut self, io: &dyn AvatarIo, avatar: Option<u16>) -> bool {
        let Some((file_slot, s)) = self.working.as_mut() else { return false };
        match avatar {
            Some(a) => {
                let mut any = false;
                for slot in 0..SLOTS as u8 {
                    if let Some(id) = s.items.iter().find(|i| i.avatar == a && i.slot == slot).map(|i| i.id) {
                        any |= s.wear(slot, Some(id));
                    }
                }
                if !any {
                    return false;
                }
            }
            None => s.wear = [None; SLOTS],
        }
        io.note(false, format!("avatar: slot {file_slot} wears {:?}", s.worn_avatars()));
        io.publish(self.working.as_ref().map(|(_, s)| s));
        true
    }
}

// ── the files ────────────────────────────────────────────────────────────────

/// The two per-slot records in one directory (`<data>/<aid>/avatar/` in the app, a directory
/// the probe is given). Every write is atomic (temp + rename), so a record is always either
/// the old one or the new one.
#[derive(Clone, Debug)]
pub struct AvatarFiles {
    pub dir: std::path::PathBuf,
}

impl AvatarFiles {
    pub fn new(dir: std::path::PathBuf) -> Self {
        Self { dir }
    }

    pub fn committed_path(&self, slot: u8) -> std::path::PathBuf {
        self.dir.join(slot.to_string())
    }

    pub fn pending_path(&self, slot: u8) -> std::path::PathBuf {
        self.dir.join(format!("{slot}.pending"))
    }

    /// `(committed, pending)` — a record that is not there is `None`; one that is there but
    /// cannot be read (an I/O error, bytes that are not UTF-8) or decoded is `None` AND an entry
    /// in `errors` (the caller logs them; the grant refuses to write over them).
    pub fn read(&self, slot: u8, errors: &mut Vec<String>) -> (Option<Record>, Option<Record>) {
        let mut one = |path: std::path::PathBuf| -> Option<Record> {
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
                Err(e) => {
                    errors.push(format!("{path:?}: {e}"));
                    return None;
                }
            };
            match decode_record(&text) {
                Ok(r) => Some(r),
                Err(e) => {
                    errors.push(format!("{path:?}: {e}"));
                    None
                }
            }
        };
        let c = one(self.committed_path(slot));
        let p = one(self.pending_path(slot));
        (c, p)
    }

    fn write_atomic(&self, path: &std::path::Path, text: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("record");
        let tmp = path.with_file_name(format!(".{name}.tmp"));
        std::fs::write(&tmp, text.as_bytes())?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    pub fn write_committed(&self, slot: u8, bind: Bind, state: &AvatarState) -> std::io::Result<()> {
        self.write_atomic(&self.committed_path(slot), &encode_record(RecordKind::Committed, bind, state))
    }

    pub fn write_pending(&self, slot: u8, base: Bind, state: &AvatarState) -> std::io::Result<()> {
        self.write_atomic(&self.pending_path(slot), &encode_record(RecordKind::Pending, base, state))
    }

    pub fn remove_pending(&self, slot: u8) {
        let _ = std::fs::remove_file(self.pending_path(slot));
    }

    /// Both records gone — the slot loads as LEGACY (a vault generation from before costumes).
    pub fn remove_all(&self, slot: u8) {
        let _ = std::fs::remove_file(self.committed_path(slot));
        self.remove_pending(slot);
    }
}

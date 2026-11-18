//! ★ 2026-10-16 「보스 레이드」 — the temple's raid gate: which 신전 the game gets when it asks for `/m/m/15.m.gz`.
//!
//! The user (2026-10-16): the upper-right pillar at the temple's entrance becomes a 귀환문-looking gate whose OK starts a
//! boss raid — *"외형이 바뀌는 기준은 가디언 모드 하드난이도를 클리어하면 그떄만 바뀌는거야. 클리어가 되지않으면 바뀌면안돼"*.
//! The game cannot make a tile conditional, so there are two 신전 files: the shipped one (`P/m/m/15.m.gz`, untouched) and
//! `P/m/m/112.m.gz` (`tools_re/raid_gate.py`: the gate's tiles · cornice · red flame · the talk trigger → map 111). The game
//! never asks for 112 — this module hands its bytes out in place of 15's while the gate is ON, through the platform
//! filesystem the overlay consults first (`filesystem.rs`: exists · size · read). The map number stays 15 everywhere
//! (`bf.b`, the save, the return point, every quest), so nothing else knows.
//!
//! ON follows the character:
//! * **the `/g<slot>` read** that starts a load ([`on_save_read`]): the committed record bound to exactly that save holds a
//!   GUARDIAN HARD receipt (`avatar_state::raid_gate_for_save`) — the temple read a moment later in the same load sees it.
//!   The slot screen's previews read every `/g` too; the last read before the map is the loaded slot's.
//! * **world entry** (`emulator.rs`): the book's answer, `false` for a new game (no `/g` read happened).
//! * **the clear hook**: a HARD guardian clear turns it on — the next temple load (the way back from 가디언) shows the gate.
//! * **the shell**: off, so nothing carries to the next load.
//!
//! Nothing here is written to a record or a save: the receipt is the costume book's, the map is data.
use std::{
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

static ON: AtomicBool = AtomicBool::new(false);
static VARIANT: OnceLock<Vec<u8>> = OnceLock::new();

/// The bytes of `P/m/m/112.m.gz` as the asset carries them — once, at boot. `false` when the asset has none (an asset from
/// before the raid): the gate then never shows, and the game runs the shipped 신전 as before.
pub fn install_variant(bytes: Vec<u8>) -> bool {
    VARIANT.set(bytes).is_ok()
}

pub fn set(on: bool) {
    if ON.swap(on, Ordering::SeqCst) != on {
        log::info!("raid gate: {}", if on { "ON — 신전 loads with the raid gate (a GUARDIAN HARD receipt)" } else { "off — the shipped 신전" });
    }
}

pub fn on() -> bool {
    ON.load(Ordering::SeqCst)
}

/// The bytes to serve for a guest path, when it is the temple and the gate is ON.
pub fn serve(path: &str) -> Option<&'static [u8]> {
    if on() && crate::guest_build::h2_raid_gate_path(path) { VARIANT.get().map(Vec::as_slice) } else { None }
}

/// A save `/g<slot>` was just read whole (`g`): decide the gate from the record bound to it.
pub fn on_save_read(data_dir: &Path, slot: u8, g: &[u8]) {
    let Some(files) = crate::vault::avatar_files(data_dir) else {
        set(false);
        return;
    };
    set(crate::avatar_state::raid_gate_for_save(&files, slot, g));
}

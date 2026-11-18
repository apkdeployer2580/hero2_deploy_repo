// ★★ 2026-10-16 정렬 · 일괄판매 — the host side of `guest_anchors::{BagSort, BulkSell}`.
//
// `include!`d by the app's worker (`emulator.rs`) AND by `wie_probe`, so the headless run drives the same steps the
// phone does: the probe's `--h2-tools` hands it the same keys and taps. The two hosts differ only in their doors to
// the guest — the app's one call gate and one write gate (`java_call` / `java_write`, the `java call:` / `java write:`
// log lines), the probe's `h2_call` — so those come in through [`BagToolsIo`].
//
// ⛔ No `use` at the top: the file lands inside `emulator.rs` (which imports `Arc`, `Emulator`, `KeyCode` …) and inside
// the probe's crate root (which imports them too); every path below is written out.

/// The receiver of a call the tools make: none (a static or a constructor), a handle just read, or a game static that
/// overrides the call's own singleton (the shop table for [`crate::guest_build::JavaCall::TableRows3`]).
#[derive(Clone, Copy, Debug)]
pub enum BtThis {
    None,
    Handle(u32),
    Static(&'static str, &'static str, &'static str),
}

/// The host's two doors to the guest — and its answer to "is this hero2" (the app asks the booted build, the probe its
/// own `build()`).
pub trait BagToolsIo {
    fn hero2(&self) -> Option<crate::guest_build::Hero2Java>;
    fn call(&mut self, emu: &mut dyn wie_backend::Emulator, what: crate::guest_build::JavaCall, this: BtThis, args: &[wie_backend::JavaArg]) -> Option<std::sync::Arc<wie_backend::GuestCallResult>>;
    fn write(&mut self, emu: &mut dyn wie_backend::Emulator, what: crate::guest_build::JavaWrite, addr: u32, bytes: &[u8]) -> bool;
}

/// What is in flight or up. Every async step carries the polls it has waited — a call that never completes (the shop
/// closed under it) gives up instead of holding the keys for ever.
#[derive(Default)]
enum BtStep {
    #[default]
    Idle,
    /// The labels `byte[][]` is being made — the grade list's (`ask` None) or the confirm's (`ask` = the grade).
    Labels { out: std::sync::Arc<wie_backend::GuestCallResult>, sell: u32, ask: Option<u8>, polls: u32 },
    /// `cl.a(BI[[B)V` is hanging the popup.
    Hang { out: std::sync::Arc<wie_backend::GuestCallResult>, sell: u32, ask: Option<u8>, labels: u32, polls: u32 },
    /// The grade list is up on the sell window.
    Grades { popup: u32, sell: u32 },
    /// The 예/아니오 box is up for `grade`.
    Ask { popup: u32, sell: u32, grade: u8 },
    /// `ag.g(I)V` is paying `sale.gold`.
    Pay { out: std::sync::Arc<wie_backend::GuestCallResult>, sell: u32, sale: crate::guest_build::H2BulkSale, polls: u32 },
    /// `new ba(cl, text)` — the game's box with the outcome.
    Note { out: std::sync::Arc<wie_backend::GuestCallResult>, sell: u32, polls: u32 },
}

/// A step that has waited this many polls gives up.
const BT_PATIENCE: u32 = 120;

#[derive(Default)]
pub struct BagTools {
    step: BtStep,
    /// The key-ups owed for key-downs the tools took.
    swallow_up: Vec<wie_backend::KeyCode>,
    /// The buttons last handed to the UI.
    shown: Vec<crate::guest_build::H2Patch>,
    /// The outcomes, for the logs and the probe: sorts made, and the last sale `(grade, records, gold)`.
    pub sorts: u32,
    pub last_sale: Option<(u8, usize, i64)>,
}

impl BagTools {
    pub fn busy(&self) -> bool {
        !matches!(self.step, BtStep::Idle)
    }

    fn rd(emu: &dyn wie_backend::Emulator) -> impl Fn(u32, usize) -> Vec<u8> + '_ {
        move |a, n| emu.read_memory(a, n)
    }

    /// Advance what is in flight; `Some(buttons)` when what the UI should draw changed.
    pub fn poll(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo) -> Option<Vec<crate::guest_build::H2Patch>> {
        let Some(h2) = io.hero2() else { return None };
        self.advance(emu, io, &h2);
        let want = self.buttons(&*emu, &h2);
        (want != self.shown).then(|| {
            self.shown = want.clone();
            want
        })
    }

    /// The buttons that belong on screen now: 정렬 on the inventory window when nothing is over it, 일괄판매 on the sell
    /// window when nothing is over it — and neither while anything of the tools is in flight.
    fn buttons(&mut self, emu: &dyn wie_backend::Emulator, h2: &crate::guest_build::Hero2Java) -> Vec<crate::guest_build::H2Patch> {
        if self.busy() {
            return Vec::new();
        }
        let read = Self::rd(emu);
        if let Some(v) = crate::guest_build::h2_inventory_window(&read, h2).filter(|v| v.on_top) {
            return vec![crate::guest_build::h2_sort_button_patch(v.panel)];
        }
        if let Some(s) = crate::guest_build::h2_sell_window(&read).filter(|s| crate::guest_build::h2_sell_idle(&read, s)) {
            return vec![crate::guest_build::h2_bulk_button_patch(s.panel)];
        }
        Vec::new()
    }

    /// A key while the tools own the screen: OK / CLR on their popup (UP / DOWN / LEFT / RIGHT go on to it — the game
    /// walks its own cursor), every key while a call is in flight. `true` = taken.
    pub fn key(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, code: wie_backend::KeyCode, down: bool) -> bool {
        use wie_backend::KeyCode;
        if !down {
            if let Some(i) = self.swallow_up.iter().position(|k| *k == code) {
                self.swallow_up.remove(i);
                return true;
            }
            return false;
        }
        let Some(h2) = io.hero2() else { return false };
        match self.step {
            BtStep::Idle => false,
            BtStep::Grades { popup, sell } | BtStep::Ask { popup, sell, .. } => {
                if !self.popup_live(&*emu, popup, sell) {
                    log::info!("bag tools: the popup {popup:#x} is gone — the key is the game's");
                    self.step = BtStep::Idle;
                    return false;
                }
                let ok = matches!(code, KeyCode::OK | KeyCode::NUM5);
                let clr = code == KeyCode::CLEAR;
                if !ok && !clr {
                    return false;
                }
                self.swallow_up.push(code);
                if ok {
                    // `j.h` — a sign-extended word like every `B` field, its low byte what a tap's focus writes.
                    let cur = {
                        let read = Self::rd(&*emu);
                        crate::guest_build::h2_fields(&read, popup)
                            .and_then(|f| crate::guest_build::h2_rd_u32(&read, f + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF))
                            .map_or(-1, |v| i32::from(v as u8 as i8))
                    };
                    self.choose(emu, io, &h2, cur);
                } else {
                    self.close_popup(emu, io, sell);
                    self.step = BtStep::Idle;
                    log::info!("bag tools: popup cancelled");
                }
                true
            }
            _ => {
                if crate::guest_build::h2_shop_up(&Self::rd(&*emu)) {
                    self.swallow_up.push(code);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// A tap at framebuffer `(fx, fy)`. `true` = the tools took it.
    pub fn tap(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, fx: u32, fy: u32) -> bool {
        let Some(h2) = io.hero2() else { return false };
        match self.step {
            BtStep::Idle => {}
            BtStep::Grades { popup, sell } | BtStep::Ask { popup, sell, .. } => {
                if !self.popup_live(&*emu, popup, sell) {
                    self.step = BtStep::Idle;
                    return false;
                }
                let tap = {
                    let read = Self::rd(&*emu);
                    let Some(s) = crate::guest_build::h2_sell_window(&read) else { return true };
                    let Some(p) = s.child else { return true };
                    crate::guest_build::h2_shop_popup_tap(&read, &h2, &p, &s.chain, fx, fy)
                };
                match tap {
                    crate::guest_build::H2PopTap::Focus(writes) => {
                        for (a, b) in &writes {
                            io.write(emu, crate::guest_build::JavaWrite::WidgetCursor, *a, b);
                        }
                    }
                    crate::guest_build::H2PopTap::Choose(row) => self.choose(emu, io, &h2, i32::from(row)),
                    crate::guest_build::H2PopTap::Swallow => {}
                }
                return true;
            }
            _ => return crate::guest_build::h2_shop_up(&Self::rd(&*emu)),
        }
        // Idle: the two buttons.
        let (sort, sell) = {
            let read = Self::rd(&*emu);
            (
                crate::guest_build::h2_inventory_window(&read, &h2).filter(|v| v.on_top),
                crate::guest_build::h2_sell_window(&read).filter(|s| crate::guest_build::h2_sell_idle(&read, s)),
            )
        };
        if let Some(v) = sort {
            // Three rows of slack above and below (the bar is 14 rows, the grid starts four below it), eight sideways
            // (the 소지금 label and the gold are not tappable).
            if crate::guest_build::h2_tool_hit(crate::guest_build::h2_tool_rect(v.panel, crate::guest_build::BagSort::BUTTON), (8, 3, 8, 3), fx, fy) {
                self.sort(emu, io, &h2, v.bag, &v);
                return true;
            }
        }
        if let Some(s) = sell {
            // No slack upward — the list's last row ends right above; some below and sideways.
            if crate::guest_build::h2_tool_hit(crate::guest_build::h2_tool_rect(s.panel, crate::guest_build::BulkSell::BUTTON), (4, 0, 6, 6), fx, fy) {
                self.open_grades(emu, io, &s);
                return true;
            }
        }
        false
    }

    fn popup_live(&self, emu: &dyn wie_backend::Emulator, popup: u32, sell: u32) -> bool {
        let read = Self::rd(emu);
        crate::guest_build::h2_sell_window(&read).is_some_and(|s| s.win.handle == sell && s.child.is_some_and(|c| c.handle == popup))
    }

    // ── 정렬 ──────────────────────────────────────────────────────────────────────────────────────────

    fn sort(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, h2: &crate::guest_build::Hero2Java, bag: u32, v: &crate::guest_build::H2InvWin) {
        let plan = {
            let read = Self::rd(&*emu);
            crate::guest_build::h2_bag_sort_plan(&read, bag).map(|(sorted, writes)| {
                let chain = crate::guest_build::h2_window_chain(&read, h2).unwrap_or_default();
                (sorted.len(), writes, crate::guest_build::h2_repaint_writes(&chain))
            })
        };
        let Some((n, writes, repaint)) = plan else {
            log::warn!("bag tools: 정렬 refused — the bag {bag:#x} could not be read whole");
            return;
        };
        let mut ok = true;
        for (a, b) in &writes {
            ok &= io.write(emu, crate::guest_build::JavaWrite::BagOrder, *a, b);
        }
        for (a, b) in &repaint {
            io.write(emu, crate::guest_build::JavaWrite::WidgetCursor, *a, b);
        }
        self.sorts += 1;
        log::info!("bag tools: 정렬 — bag {bag:#x} ({n} record(s)), {} slot(s) rewritten (ok={ok}), window {:#x}", writes.len(), v.win.handle);
    }

    // ── 일괄판매 ────────────────────────────────────────────────────────────────────────────────────────

    fn open_grades(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, s: &crate::guest_build::H2SellWin) {
        let r0 = crate::guest_build::BulkSell::GRADE_ROW0 as i32;
        let args = [r0, r0 + 1, r0 + 2, r0 + 3].map(wie_backend::JavaArg::Int);
        match io.call(emu, crate::guest_build::JavaCall::ItemTypeRows4, BtThis::None, &args) {
            Some(out) => {
                log::info!("bag tools: 일괄판매 — the grade list on sell window {:#x}", s.win.handle);
                self.step = BtStep::Labels { out, sell: s.win.handle, ask: None, polls: 0 };
            }
            None => log::warn!("bag tools: 일괄판매 — the grade names could not be asked for"),
        }
    }

    fn open_ask(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, sell: u32, grade: u8) {
        let rows_ok = crate::guest_build::h2_shop_rows(&Self::rd(&*emu)).is_some_and(|n| n > crate::guest_build::BulkSell::SHOP_ROWS_ASK[2]);
        if !rows_ok {
            // An asset from before the confirm lines: a sale of this value is not made without the question.
            log::warn!("bag tools: 일괄판매 — the shop table has no confirm lines (asset not regenerated?); grade {grade} not sold");
            self.step = BtStep::Idle;
            return;
        }
        let args = crate::guest_build::BulkSell::SHOP_ROWS_ASK.map(|r| wie_backend::JavaArg::Int(r as i32));
        match io.call(emu, crate::guest_build::JavaCall::TableRows3, BtThis::Static("v", "a", "Li;"), &args) {
            Some(out) => self.step = BtStep::Labels { out, sell, ask: Some(grade), polls: 0 },
            None => {
                log::warn!("bag tools: 일괄판매 — the confirm lines could not be asked for");
                self.step = BtStep::Idle;
            }
        }
    }

    /// An OK (key, or a second tap) on the tools' popup: `row` is its cursor.
    fn choose(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, h2: &crate::guest_build::Hero2Java, row: i32) {
        match std::mem::take(&mut self.step) {
            BtStep::Grades { sell, .. } => {
                self.close_popup(emu, io, sell);
                let Ok(grade) = u8::try_from(row) else { return };
                if grade >= crate::guest_build::BulkSell::GRADES {
                    return;
                }
                log::info!("bag tools: 일괄판매 — grade {grade} chosen");
                if grade >= crate::guest_build::BulkSell::ASK_FROM {
                    self.open_ask(emu, io, sell, grade);
                } else {
                    self.sell(emu, io, h2, sell, grade);
                }
            }
            BtStep::Ask { sell, grade, .. } => {
                self.close_popup(emu, io, sell);
                if row == 0 {
                    log::info!("bag tools: 일괄판매 — grade {grade} confirmed");
                    self.sell(emu, io, h2, sell, grade);
                } else {
                    log::info!("bag tools: 일괄판매 — grade {grade} declined");
                }
            }
            other => self.step = other,
        }
    }

    /// The popup off the sell window, at once — the game's own `j.b(I)V`: the child ← null, the repaint flags, `n.b()`.
    fn close_popup(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, sell: u32) {
        let writes = {
            let read = Self::rd(&*emu);
            crate::guest_build::h2_sell_window(&read).filter(|s| s.win.handle == sell).map(|s| crate::guest_build::h2_child_open_writes(&s.win, 0, &s.chain))
        };
        for (kind, a, b) in writes.unwrap_or_default() {
            io.write(emu, kind, a, &b);
        }
        let _ = io.call(emu, crate::guest_build::JavaCall::HudDirty, BtThis::None, &[]);
    }

    fn sell(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, h2: &crate::guest_build::Hero2Java, sell: u32, grade: u8) {
        let sale = {
            let read = Self::rd(&*emu);
            crate::guest_build::h2_sell_window(&read).filter(|s| s.win.handle == sell).and_then(|s| crate::guest_build::h2_bulk_sell_plan(&read, h2, &s, grade))
        };
        let Some(sale) = sale else {
            log::warn!("bag tools: 일괄판매 — the bag could not be read whole; nothing sold");
            self.step = BtStep::Idle;
            return;
        };
        if sale.sold.is_empty() {
            log::info!("bag tools: 일괄판매 — grade {grade}: nothing to sell");
            self.note(emu, io, sell, crate::guest_build::BulkSell::SHOP_ROW_NONE);
            return;
        }
        let gold = crate::guest_build::h2_bulk_gold_arg(&sale);
        log::info!("bag tools: 일괄판매 — grade {grade}: {} record(s) for {} (paying first)", sale.sold.len(), sale.gold);
        match io.call(emu, crate::guest_build::JavaCall::GoldAdd, BtThis::None, &[wie_backend::JavaArg::Int(gold)]) {
            Some(out) => self.step = BtStep::Pay { out, sell, sale, polls: 0 },
            None => {
                log::warn!("bag tools: 일괄판매 — ag.g(I)V refused; nothing sold");
                self.step = BtStep::Idle;
            }
        }
    }

    fn note(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, sell: u32, row: u32) {
        let text = {
            let read = Self::rd(&*emu);
            crate::guest_build::h2_shop_row_handle(&read, row).or_else(|| {
                (row == crate::guest_build::BulkSell::SHOP_ROW_NONE).then(|| crate::guest_build::h2_shop_row_handle(&read, crate::guest_build::BulkSell::SHOP_ROW_NONE_SHIPPED)).flatten()
            })
        };
        let Some(text) = text else {
            log::warn!("bag tools: shop row {row} not there — no box");
            self.step = BtStep::Idle;
            return;
        };
        let args = [wie_backend::JavaArg::Object(sell), wie_backend::JavaArg::Object(text)];
        match io.call(emu, crate::guest_build::JavaCall::MessageBoxNew, BtThis::None, &args) {
            Some(out) => self.step = BtStep::Note { out, sell, polls: 0 },
            None => self.step = BtStep::Idle,
        }
    }

    fn advance(&mut self, emu: &mut dyn wie_backend::Emulator, io: &mut dyn BagToolsIo, h2: &crate::guest_build::Hero2Java) {
        match std::mem::take(&mut self.step) {
            BtStep::Idle => {}
            BtStep::Grades { popup, sell } => {
                if self.popup_live(&*emu, popup, sell) {
                    self.step = BtStep::Grades { popup, sell };
                }
            }
            BtStep::Ask { popup, sell, grade } => {
                if self.popup_live(&*emu, popup, sell) {
                    self.step = BtStep::Ask { popup, sell, grade };
                }
            }
            BtStep::Labels { out, sell, ask, polls } => match out.poll() {
                None if polls < BT_PATIENCE => self.step = BtStep::Labels { out, sell, ask, polls: polls + 1 },
                None => log::warn!("bag tools: the labels never came — given up"),
                Some(Err(())) | Some(Ok(0)) => log::warn!("bag tools: the labels call failed"),
                Some(Ok(labels)) => {
                    let (kind, rows) = match ask {
                        None => (crate::guest_build::BulkSell::LIST_KIND, i32::from(crate::guest_build::BulkSell::GRADES)),
                        Some(_) => (crate::guest_build::BulkSell::ASK_KIND, crate::guest_build::BulkSell::ASK_BUTTONS),
                    };
                    let live = crate::guest_build::h2_sell_window(&Self::rd(&*emu)).is_some_and(|s| s.win.handle == sell && s.child.is_none());
                    let args = [wie_backend::JavaArg::Int(i32::from(kind)), wie_backend::JavaArg::Int(rows), wie_backend::JavaArg::Object(labels)];
                    match live.then(|| io.call(emu, crate::guest_build::JavaCall::SellPopupNew, BtThis::Handle(sell), &args)).flatten() {
                        Some(out) => self.step = BtStep::Hang { out, sell, ask, labels, polls: 0 },
                        None => {
                            log::info!("bag tools: the sell window moved on before its popup — dropped");
                            emu.release_java_object(labels);
                        }
                    }
                }
            },
            BtStep::Hang { out, sell, ask, labels, polls } => match out.poll() {
                None if polls < BT_PATIENCE => self.step = BtStep::Hang { out, sell, ask, labels, polls: polls + 1 },
                None => log::warn!("bag tools: the popup never hung — given up"),
                Some(done) => {
                    // The popup holds the labels now (`d.b`); the host's pin goes.
                    emu.release_java_object(labels);
                    let popup = {
                        let read = Self::rd(&*emu);
                        crate::guest_build::h2_sell_window(&read).filter(|s| s.win.handle == sell).and_then(|s| s.child).filter(|c| c.class == h2.classes.popup)
                    };
                    match (done, popup) {
                        (Ok(_), Some(p)) => {
                            log::info!("bag tools: {} up as {:#x}", if ask.is_some() { "the confirm box" } else { "the grade list" }, p.handle);
                            self.step = match ask {
                                None => BtStep::Grades { popup: p.handle, sell },
                                Some(grade) => BtStep::Ask { popup: p.handle, sell, grade },
                            };
                        }
                        _ => log::warn!("bag tools: the popup did not hang"),
                    }
                }
            },
            BtStep::Pay { out, sell, sale, polls } => match out.poll() {
                None if polls < BT_PATIENCE => self.step = BtStep::Pay { out, sell, sale, polls: polls + 1 },
                None => log::error!("bag tools: 일괄판매 — ag.g(I)V never returned; the records stay (gold state unknown)"),
                Some(Err(())) => log::error!("bag tools: 일괄판매 — ag.g(I)V failed; nothing sold"),
                Some(Ok(_)) => {
                    let writes = {
                        let read = Self::rd(&*emu);
                        crate::guest_build::h2_sell_window(&read).filter(|s| s.win.handle == sell).and_then(|s| crate::guest_build::h2_bulk_sell_writes(&read, h2, &s, &sale))
                    };
                    let Some(writes) = writes else {
                        // The bag is not the one that was priced (nothing should move it while the keys are held — this
                        // is the backstop): take the payment back through the same method, and sell nothing.
                        let back = -crate::guest_build::h2_bulk_gold_arg(&sale);
                        let refunded = io.call(emu, crate::guest_build::JavaCall::GoldAdd, BtThis::None, &[wie_backend::JavaArg::Int(back)]).is_some();
                        log::error!("bag tools: 일괄판매 — paid {} but the bag moved since the plan; nothing sold, payment taken back (issued={refunded})", sale.gold);
                        return;
                    };
                    let mut ok = true;
                    for (kind, a, b) in &writes {
                        ok &= io.write(emu, *kind, *a, b);
                    }
                    self.last_sale = Some((sale.grade, sale.sold.len(), sale.gold));
                    log::info!("bag tools: 일괄판매 — sold {} record(s) of grade {} for {} ({} write(s), ok={ok})", sale.sold.len(), sale.grade, sale.gold, writes.len());
                    self.note(emu, io, sell, crate::guest_build::BulkSell::SHOP_ROW_SOLD);
                }
            },
            BtStep::Note { out, sell, polls } => match out.poll() {
                None if polls < BT_PATIENCE => self.step = BtStep::Note { out, sell, polls: polls + 1 },
                None => log::warn!("bag tools: the box never came"),
                Some(Err(())) | Some(Ok(0)) => log::warn!("bag tools: new ba(..) failed"),
                Some(Ok(ba)) => {
                    let writes = {
                        let read = Self::rd(&*emu);
                        crate::guest_build::h2_sell_window(&read).filter(|s| s.win.handle == sell).map(|s| crate::guest_build::h2_child_open_writes(&s.win, ba, &s.chain))
                    };
                    for (kind, a, b) in writes.unwrap_or_default() {
                        io.write(emu, kind, a, &b);
                    }
                    emu.release_java_object(ba);
                }
            },
        }
    }
}

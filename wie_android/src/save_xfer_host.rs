// ★★ 2026-10-28 — 환경설정 맨 밑 세 행: 세이브내보내기 · 세이브불러오기 · 세이브되돌리기 (the worker's half).
//
// User: *"hero5와 cronossword 처럼 인게임 내에 환경설정에서 맨밑에 행 3개 추가해서 세이브 내보내기, 세이브
// 불러오기, 세이브 되돌리기 기능을 넣을꺼야. cronossword에서 작업했을때 결함이 몇가지 있어서 코드리뷰로
// 수정작업을 여러번했는데 이거 모두 파악해서 문제없도록"*. `include!`d into `emulator.rs`.
//
// The rows are `hero2java::H2_OPT_SAVE_*` (`OptionsExt` reports a press). The file and the apply are
// `savebundle` / `save_transfer` / `vault` (engine and policy); this is the dialogue:
//
//   세이브내보내기   in the world: the game's own save first (`ag.l()V`, the SAVE button's call, behind
//                   the game's own gate) and then WAIT until the host's commits that ride on a save —
//                   the warehouse blobs, the costume record bound to the new `/g`, the plate rows —
//                   are on the disk, so the file is one moment; where the game may not save, the last
//                   save goes and the notice says so. At the title the disk IS the save. Then the file
//                   → the system's 「저장 위치」 screen → the game's box: 세이브를 내보냈습니다.
//   세이브불러오기   question → the system's 「열기」 screen → the file checked (`check_import_file`) →
//                   question with its date and slots → 예: staged, the app closes; the next start applies.
//   세이브되돌리기   the newest backup (the phone just before the last 불러오기 / 되돌리기) → question with
//                   its date → 예: staged, the app closes. Twice = a round trip.
//
// ChronosSword's review fixes, as they land here (its CLAUDE.md 「코드리뷰에서 나온 것」):
//   * a file screen is a REQUEST with its own number and its own file (`savebundle::doc_file`); only
//     the answer of the request in flight is taken, any other answer only removes its own file; a press
//     while one is in flight is refused with a box (until [`XFER_PATIENCE`], after which a press is a new
//     request and the old answer is harmless); the answers are a queue (`crate::take_save_doc_results`).
//   * a late answer whose screen is gone (the 환경설정 window closed while a cloud file downloaded) is
//     refused, its file removed.
//   * the stage is the last thing before the app closes — and an `Unsure` stage closes it too
//     (`vault::Staged`), since the next start may apply it.
//   * the questions hold the guest (nothing moves under them) and take every key and tap; their
//     release halves are owed to them after they close.
//   * the notices are the game's own message box over the 환경설정 window (`new ba(window, text)`, the
//     user's standing rule: 안내는 게임 자신의 팝업으로); `OptionsExt` keeps the scrolled rows around it.

/// How long a file screen may stay unanswered before a press is a new request.
const XFER_PATIENCE: Duration = Duration::from_secs(120);
/// How long 세이브내보내기 waits for the game's save and the host's commits after it.
const XFER_SAVE_MAX: Duration = Duration::from_secs(10);
/// A question's picture is sent again this often (the guest is held under it — see `REVIVE_RESEND`).
const XFER_RESEND: Duration = Duration::from_millis(1000);
/// The game's message box wraps at 101 px (`ba.a(II)V` → `ac.a(Graphics,IIII[B)V`).
const XFER_BOX_WIDTH: i32 = 101;
/// Lines a question may have: `16 · (2 + lines)` must fit the 220-row framebuffer with room.
const XFER_ASK_LINES: usize = 10;

/// `MainActivity.DOC_*`.
const DOC_DONE: i32 = 0;
const DOC_CANCELLED: i32 = 1;
const DOC_TOO_BIG: i32 = 3;

const M_BUSY: &str = "파일을 옮기는 중입니다. 잠시 후 다시 눌러 주세요.";
/// 세이브내보내기 waited [`XFER_SAVE_MAX`] for the save or its commits.
const M_LATE: &str = "저장을 마치지 못했습니다. 잠시 후 다시 눌러 주세요.";
/// The plate store's last write did not land (`config::quickpages_write_ok`, code review P2).
const M_PAGES_UNSAVED: &str = crate::vault::PAGES_UNSAVED;

/// What a question is about.
enum XferAsk {
    /// 「Pick a file from another phone?」
    ImportPick,
    /// 「Apply this file?」 — the copy the 「열기」 screen made.
    ImportApply { file: PathBuf },
    /// 「Go back to the backup kept at `ms`?」
    Restore { ms: u64 },
}

/// A question on screen: the game's 예/아니오 box, redrawn over the held frame.
struct XferBox {
    ask: XferAsk,
    confirm: crate::guest_build::H2Confirm,
    lines: Vec<Vec<u8>>,
    /// `(ax.g, ax.i, ax.j)` when it went up.
    screen: (i32, i32, i32),
    sent: Instant,
}

/// The file screen in flight.
struct XferDoc {
    id: u32,
    export: bool,
    file: PathBuf,
    since: Instant,
    /// What the export's notice says when it is done.
    done_text: String,
}

/// The game's box being built: the text's `byte[]`, then the box, then hung on the window.
enum XferNotice {
    Bytes { out: Arc<wie_backend::GuestCallResult>, window: u32 },
    Box { out: Arc<wie_backend::GuestCallResult>, window: u32, arr: u32 },
}

/// What the loop knows about a save and the commits that ride on it (세이브내보내기 waiting).
struct XferSettled {
    /// `SaveState::java_outcome`.
    save: Option<bool>,
    /// `AccountState::commit_settled`.
    account: Option<bool>,
    /// `AvatarBook::caught_up`: `None` = a costume record's commit did not land.
    avatar: Option<bool>,
    /// The plate rows committed and written: `Some(false)` = not yet; `None` = the last write of the
    /// store failed (code review P2) — the disk holds an older store than the save beside it.
    pages: Option<bool>,
}

#[derive(Default)]
struct SaveXfer {
    modal: Option<XferBox>,
    font: Option<crate::guest_build::H2Font>,
    /// Keys whose DOWN a question took — their UP is owed to it.
    swallow_up: Vec<KeyCode>,
    doc: Option<XferDoc>,
    next_id: u32,
    /// 세이브내보내기 waiting for the disk to be one moment.
    export: Option<XferExport>,
    notice: Option<XferNotice>,
}

/// 세이브내보내기 between the press and the file.
struct XferExport {
    since: Instant,
    /// The save edges before the game's own save. `None` = no save was asked (the title, or where the
    /// game may not save): the disk goes as it is — but only once every host commit on it is settled
    /// too (code review P1: a warehouse whose write failed after the last save, carried beside that
    /// save, duplicates or loses what moved).
    edges0: Option<u64>,
    done_text: &'static str,
}

impl SaveXfer {
    /// A question is up: the guest must not tick, and every key and tap is its.
    fn holding(&self) -> bool {
        self.modal.is_some()
    }

    /// 세이브내보내기 is waiting (for the game's save, or for the host's commits): the guest runs (it may be
    /// saving), but the player's key-downs and taps are dropped — a menu key now would close the window
    /// the answer comes to.
    fn owns_input(&self) -> bool {
        self.export.is_some()
    }

    /// The 환경설정 window, wherever it is in the chain, with the chain and whether the field menu
    /// (not the title) is its root.
    fn options(emulator: &dyn Emulator) -> Option<(crate::guest_build::H2Win, Vec<crate::guest_build::H2Win>, bool)> {
        let h2 = crate::guest_build::hero2_java()?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let chain = crate::guest_build::h2_window_chain(&read, &h2)?;
        let win = *chain.iter().find(|w| w.class == h2.classes.option_window)?;
        let world = chain.first().is_some_and(|w| w.class == h2.classes.field_menu);
        Some((win, chain, world))
    }

    /// The 환경설정 window is the top of the chain (nothing of ours or the game's over it).
    fn options_open(emulator: &dyn Emulator) -> bool {
        Self::options(emulator).is_some_and(|(win, chain, _)| chain.last().is_some_and(|t| t.handle == win.handle))
    }

    fn new_id(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    /// Is a file screen still in flight? Past [`XFER_PATIENCE`] it is given up on: the next press is a
    /// new request, and the late answer, a different number, only removes its own file.
    fn doc_busy(&mut self) -> bool {
        match &self.doc {
            Some(d) if d.since.elapsed() < XFER_PATIENCE => true,
            Some(d) => {
                log::warn!("save xfer: file screen {} unanswered after {:?} — given up on", d.id, XFER_PATIENCE);
                self.doc = None;
                false
            }
            None => false,
        }
    }

    /// One of the three rows was pressed.
    fn press(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, saving: &mut SaveState, row: usize) {
        if self.export.is_some() || self.modal.is_some() {
            return;
        }
        if self.doc_busy() {
            self.say(emulator, M_BUSY);
            return;
        }
        let Some((_, _, world)) = Self::options(&*emulator) else { return };
        match row {
            crate::guest_build::H2_OPT_SAVE_EXPORT => self.press_export(emulator, saving, world),
            crate::guest_build::H2_OPT_SAVE_IMPORT => {
                let head = "다른 폰에서 내보낸 세이브 파일을 고릅니다. 불러오면 이 폰의 세이브가 모두 그 파일의 것으로 바뀝니다.";
                self.ask(emulator, proxy, XferAsk::ImportPick, head, "계속할까요?");
            }
            crate::guest_build::H2_OPT_SAVE_RESTORE => match crate::vault::newest_backup(data_dir) {
                Ok(None) => self.say(emulator, "되돌릴 세이브가 없습니다. 세이브를 불러오거나 되돌린 적이 있을 때 그 직전 상태로 되돌립니다."),
                Err(msg) => self.say(emulator, &msg),
                Ok(Some((ms, _, b))) => {
                    let mut head = format!("{} 상태로 되돌립니다.", crate::savebundle::short_stamp(ms, crate::vault::tz_offset_min()));
                    for slot in b.slots() {
                        head.push('\n');
                        head.push_str(&crate::vault::bundle_slot_box_line(&b, slot));
                    }
                    if b.slots().is_empty() {
                        head.push_str("\n(세이브 없음)");
                    }
                    self.ask(emulator, proxy, XferAsk::Restore { ms }, &head, "앱을 닫고 다음 실행 때 적용합니다. 지금 되돌릴까요?");
                }
            },
            _ => {}
        }
    }

    fn press_export(&mut self, emulator: &mut dyn Emulator, saving: &mut SaveState, world: bool) {
        if !world {
            // The title: no game is open, so the files are the save — once the commits that rode on
            // the last one are on the disk beside it.
            self.export = Some(XferExport { since: Instant::now(), edges0: None, done_text: "세이브를 내보냈습니다." });
            return;
        }
        let (gate, down) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let gate = crate::guest_build::hero2_save().is_some_and(|sv| crate::guest_build::h2_can_save_here(&read, &sv.area));
            (gate, crate::guest_build::hero2_java().is_some_and(|h2| crate::guest_build::h2_char_down(&read, &h2)))
        };
        if !gate || down {
            // The game's own refusal (a boss room): its rule stands — the last save goes, and says so.
            log::info!("save xfer: export where the game may not save — the last save goes");
            self.export = Some(XferExport {
                since: Instant::now(),
                edges0: None,
                done_text: "세이브를 내보냈습니다. 저장할 수 없는 곳이라 마지막으로 저장한 세이브입니다.",
            });
            return;
        }
        if saving.busy() {
            self.say(emulator, "저장하는 중입니다. 잠시 후 다시 눌러 주세요.");
            return;
        }
        saving.java_outcome = None;
        let edges = crate::vault::save_edges();
        if save_begin(emulator, saving, save::Reason::Auto, now_ms()) {
            log::info!("save xfer: export — the game saves first");
            self.export = Some(XferExport { since: Instant::now(), edges0: Some(edges), done_text: "저장한 뒤 세이브를 내보냈습니다." });
        } else {
            self.say(emulator, "지금은 저장할 수 없습니다. 잠시 후 다시 눌러 주세요.");
        }
    }

    /// The file of the phone's state now → the 「저장 위치」 screen.
    fn export_now(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, done_text: &str) {
        let id = self.new_id();
        let file = crate::savebundle::doc_file(data_dir, true, id);
        let aid = crate::ARCHIVE_AID.get().map(String::as_str);
        let slots = match crate::vault::export_to(data_dir, aid, crate::vault::app_version(), &file) {
            Ok(slots) => slots,
            Err(msg) => {
                let _ = std::fs::remove_file(&file);
                self.say(emulator, &msg);
                return;
            }
        };
        let (y, m, d, hh, mm) = crate::savebundle::civil(now_ms(), crate::vault::tz_offset_min());
        let name = format!("영웅서기2_세이브_{y:04}{m:02}{d:02}_{hh:02}{mm:02}.zip");
        let list = slots.iter().map(u8::to_string).collect::<Vec<_>>().join(", ");
        let done_text = format!("{done_text}\n슬롯 {list}");
        if !crate::server_prompt::save_doc(&format!("export|{id}|{}|{name}", file.display())) {
            let _ = std::fs::remove_file(&file);
            self.say(emulator, "파일 저장 화면을 열지 못했습니다.");
            return;
        }
        log::info!("save xfer: export of slots {slots:?} — file screen {id} open");
        self.doc = Some(XferDoc { id, export: true, file, since: Instant::now(), done_text });
    }

    /// The 「열기」 screen for a file from another phone.
    fn open_import(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path) {
        let id = self.new_id();
        let file = crate::savebundle::doc_file(data_dir, false, id);
        if !crate::server_prompt::save_doc(&format!("import|{id}|{}", file.display())) {
            self.say(emulator, "파일 선택 화면을 열지 못했습니다.");
            return;
        }
        log::info!("save xfer: import — file screen {id} open");
        self.doc = Some(XferDoc { id, export: false, file, since: Instant::now(), done_text: String::new() });
    }

    /// Every loop: the file screens' answers, the export's wait, the box being built, the question's resend.
    fn poll(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, settled: Option<XferSettled>) {
        for (id, status) in crate::take_save_doc_results() {
            self.answer_doc(emulator, data_dir, proxy, id, status);
        }
        if let Some(s) = settled {
            self.poll_export(emulator, data_dir, s);
        }
        self.poll_notice(emulator);
        if self.modal.as_ref().is_some_and(|m| m.sent.elapsed() >= XFER_RESEND) {
            self.send(proxy);
        }
    }

    fn answer_doc(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, id: u32, status: i32) {
        if self.doc.as_ref().is_none_or(|d| d.id != id) {
            // A request given up on: only its own files.
            for export in [true, false] {
                let _ = std::fs::remove_file(crate::savebundle::doc_file(data_dir, export, id));
            }
            log::info!("save xfer: file screen {id} answered {status} after it was given up on — its file removed");
            return;
        }
        let Some(doc) = self.doc.take() else { return };
        if doc.export {
            let _ = std::fs::remove_file(&doc.file);
            match status {
                DOC_DONE => {
                    log::warn!("save xfer: export written by file screen {id}");
                    self.say(emulator, &doc.done_text);
                }
                DOC_CANCELLED => log::info!("save xfer: export cancelled"),
                _ => self.say(emulator, "파일을 저장하지 못했습니다. 다른 곳을 골라 다시 해 주세요."),
            }
            return;
        }
        if status != DOC_DONE {
            let _ = std::fs::remove_file(&doc.file);
            match status {
                DOC_CANCELLED => log::info!("save xfer: import cancelled"),
                DOC_TOO_BIG => self.say(emulator, "영웅서기2 세이브 파일이 아닙니다(파일이 너무 큽니다)."),
                _ => self.say(emulator, "파일을 읽지 못했습니다."),
            }
            return;
        }
        // The answer is for a screen that must still be there.
        if !Self::options_open(&*emulator) {
            let _ = std::fs::remove_file(&doc.file);
            log::warn!("save xfer: the file came back after the 환경설정 window closed — dropped");
            return;
        }
        let aid = crate::ARCHIVE_AID.get().map(String::as_str);
        match crate::vault::check_import_file(data_dir, aid, crate::vault::app_version(), &doc.file) {
            Err(msg) => {
                let _ = std::fs::remove_file(&doc.file);
                self.say(emulator, &msg);
            }
            Ok((_, b)) => {
                let mut head = if b.created_ms > 0 {
                    format!("{} 내보낸 세이브", crate::savebundle::short_stamp(b.created_ms, crate::vault::tz_offset_min()))
                } else {
                    "다른 폰의 세이브".to_string()
                };
                for slot in b.slots() {
                    head.push('\n');
                    head.push_str(&crate::vault::bundle_slot_box_line(&b, slot));
                }
                let tail = "이 폰의 세이브는 백업합니다. 앱을 닫고 다음 실행 때 적용합니다. 지금 불러올까요?";
                self.ask(emulator, proxy, XferAsk::ImportApply { file: doc.file }, &head, tail);
            }
        }
    }

    fn poll_export(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, s: XferSettled) {
        let Some(x) = &self.export else { return };
        let (since, done_text) = (x.since, x.done_text);
        let late = since.elapsed() >= XFER_SAVE_MAX;
        // In the world the game's save comes first, and until it has landed what the commits say is
        // about the save before it (a failure then is one this save's commit may clear). With no save
        // asked, the commits are judged as they are: every one of them must have landed.
        let saved = match x.edges0 {
            None => true,
            Some(e0) => s.save == Some(true) && crate::vault::save_edges() != e0,
        };
        let fail = if x.edges0.is_some() && s.save == Some(false) {
            Some("저장하지 못해 내보내지 않았습니다.")
        } else if !saved {
            if late { Some(M_LATE) } else { return }
        } else if s.account.is_none() {
            Some(crate::vault::ACCOUNT_UNSAVED)
        } else if s.avatar.is_none() {
            Some(crate::vault::AVATAR_UNSAVED)
        } else if s.pages.is_none() {
            Some(M_PAGES_UNSAVED)
        } else if s.account == Some(true) && s.avatar == Some(true) && s.pages == Some(true) {
            None
        } else if late {
            Some(M_LATE)
        } else {
            return;
        };
        self.export = None;
        match fail {
            Some(msg) => {
                log::warn!("save xfer: export refused — {msg} (save {:?}, account {:?}, avatar {:?}, pages {:?})", s.save, s.account, s.avatar, s.pages);
                self.say(emulator, msg);
            }
            None => {
                log::info!("save xfer: the saves and their commits are on the disk ({:?}) — exporting", since.elapsed());
                self.export_now(emulator, data_dir, done_text);
            }
        }
    }

    /// Raise a question: `head` (what it is about — a date, the slots) then `tail` (the question),
    /// each broken into the box's lines. The tail always shows whole; a head too long for the box
    /// loses its last lines to a `...`.
    fn ask(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, ask: XferAsk, head: &str, tail: &str) {
        let Some(h2) = crate::guest_build::hero2_java() else { return };
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        if self.font.is_none() {
            self.font = crate::guest_build::h2_font_from_guest(&read);
        }
        let screen = (crate::guest_build::h2_rd_u32(&read, h2.screen.0), crate::guest_build::h2_rd_u32(&read, h2.screen.1), crate::guest_build::h2_rd_u32(&read, h2.screen.2));
        let (Some(sw), Some(sh), Some(cx)) = screen else { return };
        if self.font.is_none() {
            log::warn!("save xfer: the game's font is not readable — no question");
            if let XferAsk::ImportApply { file } = &ask {
                let _ = std::fs::remove_file(file);
            }
            return;
        }
        let tail = crate::guest_build::h2_box_wrap(tail, XFER_BOX_WIDTH);
        let mut head = crate::guest_build::h2_box_wrap(head, XFER_BOX_WIDTH);
        let room = XFER_ASK_LINES.saturating_sub(tail.len());
        if head.len() > room {
            head.truncate(room.saturating_sub(1));
            head.push("...".to_string());
        }
        let lines: Vec<Vec<u8>> = head.iter().chain(tail.iter()).map(|l| euc_lossy(l)).collect();
        self.modal = Some(XferBox { ask, confirm: crate::guest_build::H2Confirm::new(), lines, screen: (sw as i32, sh as i32, cx as i32), sent: Instant::now() });
        self.send(proxy);
    }

    fn send(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        let (Some(m), Some(font)) = (self.modal.as_mut(), self.font.as_ref()) else { return };
        let (sw, sh, cx) = m.screen;
        let patch = crate::guest_build::h2_confirm_patch(font, &m.lines, m.confirm.yes, sw, sh, cx);
        m.sent = Instant::now();
        let _ = proxy.send_event(UserEvent::RevivePrompt(vec![patch]));
    }

    /// A key. While a question is up every DOWN is taken and so is its UP. `true` = taken.
    fn key(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, code: KeyCode, down: bool) -> bool {
        if !down {
            if let Some(i) = self.swallow_up.iter().position(|k| *k == code) {
                self.swallow_up.remove(i);
                return true;
            }
            return false;
        }
        let Some(m) = self.modal.as_mut() else { return false };
        if !self.swallow_up.contains(&code) {
            self.swallow_up.push(code);
        }
        use crate::guest_build::H2OptDir;
        let dir = match code {
            KeyCode::UP | KeyCode::NUM2 => Some(H2OptDir::Up),
            KeyCode::DOWN | KeyCode::NUM8 => Some(H2OptDir::Down),
            KeyCode::LEFT | KeyCode::NUM4 => Some(H2OptDir::Left),
            KeyCode::RIGHT | KeyCode::NUM6 => Some(H2OptDir::Right),
            KeyCode::OK | KeyCode::NUM5 => Some(H2OptDir::Ok),
            // CLR answers 아니오 here: nothing irreversible hides behind it (unlike the revive box).
            KeyCode::CLEAR => {
                self.answer(emulator, data_dir, proxy, false);
                return true;
            }
            _ => None,
        };
        let act = m.confirm.key(dir);
        self.act(emulator, data_dir, proxy, act);
        true
    }

    /// A tap while a question is up — always taken.
    fn tap(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, fx: u32, fy: u32) -> bool {
        let Some(m) = self.modal.as_mut() else { return false };
        let (sw, sh, cx) = m.screen;
        let hit = crate::guest_build::h2_confirm_hit(m.lines.len(), sw, sh, cx, fx as i32, fy as i32);
        let act = m.confirm.tap(hit);
        self.act(emulator, data_dir, proxy, act);
        true
    }

    fn act(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, act: crate::guest_build::H2ConfirmAct) {
        match act {
            crate::guest_build::H2ConfirmAct::Moved => self.send(proxy),
            crate::guest_build::H2ConfirmAct::Swallowed => {}
            crate::guest_build::H2ConfirmAct::Chose(yes) => self.answer(emulator, data_dir, proxy, yes),
        }
    }

    fn answer(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, yes: bool) {
        let Some(m) = self.modal.take() else { return };
        let _ = proxy.send_event(UserEvent::RevivePrompt(Vec::new()));
        match m.ask {
            XferAsk::ImportPick => {
                if yes {
                    self.open_import(emulator, data_dir);
                }
            }
            XferAsk::ImportApply { file } => {
                if !yes {
                    let _ = std::fs::remove_file(&file);
                    log::info!("save xfer: import declined");
                    return;
                }
                // Checked again: what is staged is what is read now.
                let aid = crate::ARCHIVE_AID.get().map(String::as_str);
                let staged = crate::vault::check_import_file(data_dir, aid, crate::vault::app_version(), &file).and_then(|(bytes, _)| crate::vault::stage_bytes(data_dir, &bytes, "import"));
                let _ = std::fs::remove_file(&file);
                match staged {
                    Ok(how) => quit_for_apply(how),
                    Err(msg) => self.say(emulator, &msg),
                }
            }
            XferAsk::Restore { ms } => {
                if !yes {
                    return;
                }
                match crate::vault::stage_restore_newest(data_dir, ms) {
                    Ok(how) => quit_for_apply(how),
                    Err(msg) => self.say(emulator, &msg),
                }
            }
        }
    }

    /// Say `text` in the game's own message box over the 환경설정 window (dropped, logged, when the
    /// window is not the top any more, or another box is being built).
    fn say(&mut self, emulator: &mut dyn Emulator, text: &str) {
        if self.notice.is_some() {
            log::info!("save xfer: a box is being built — {text:?} not shown");
            return;
        }
        let Some((win, chain, _)) = Self::options(&*emulator) else {
            log::info!("save xfer: no 환경설정 window — {text:?} not shown");
            return;
        };
        if chain.last().is_none_or(|t| t.handle != win.handle) {
            log::info!("save xfer: something is over the 환경설정 window — {text:?} not shown");
            return;
        }
        let bytes = euc_lossy(&crate::guest_build::h2_box_wrap(text, XFER_BOX_WIDTH).join(";"));
        let out = Arc::new(wie_backend::GuestCallResult::new());
        emulator.new_java_byte_array(&bytes, out.clone());
        log::info!("save xfer: notice {text:?}");
        self.notice = Some(XferNotice::Bytes { out, window: win.handle });
    }

    fn poll_notice(&mut self, emulator: &mut dyn Emulator) {
        match self.notice.take() {
            None => {}
            Some(XferNotice::Bytes { out, window }) => match out.poll() {
                None => self.notice = Some(XferNotice::Bytes { out, window }),
                Some(Err(())) | Some(Ok(0)) => log::warn!("save xfer: no byte[] for the notice"),
                Some(Ok(arr)) => {
                    let args = [wie_backend::JavaArg::Object(window), wie_backend::JavaArg::Object(arr)];
                    match java_call(emulator, crate::guest_build::JavaCall::MessageBoxNew, JavaThis::None, &args) {
                        Some(out) => self.notice = Some(XferNotice::Box { out, window, arr }),
                        None => {
                            log::warn!("save xfer: new ba(..) refused");
                            emulator.release_java_object(arr);
                        }
                    }
                }
            },
            Some(XferNotice::Box { out, window, arr }) => match out.poll() {
                None => self.notice = Some(XferNotice::Box { out, window, arr }),
                Some(Err(())) | Some(Ok(0)) => {
                    log::warn!("save xfer: new ba(..) failed");
                    emulator.release_java_object(arr);
                }
                Some(Ok(bx)) => {
                    // Hung only on the same window, still the top.
                    let writes = Self::options(&*emulator)
                        .filter(|(win, chain, _)| win.handle == window && chain.last().is_some_and(|t| t.handle == window))
                        .map(|(win, chain, _)| crate::guest_build::h2_child_open_writes(&win, bx, &chain));
                    match writes {
                        Some(writes) => {
                            let mut it = writes.iter();
                            let hung = it.next().is_some_and(|(kind, a, bytes)| java_write(emulator, *kind, *a, bytes));
                            for (kind, a, bytes) in it {
                                java_write(emulator, *kind, *a, bytes);
                            }
                            log::info!("save xfer: the game's box {bx:#x} over the 환경설정 window (hung={hung})");
                        }
                        None => log::info!("save xfer: the 환경설정 window changed before the box was ready — not shown"),
                    }
                    emulator.release_java_object(bx);
                    emulator.release_java_object(arr);
                }
            },
        }
    }
}

/// A line of text the game's font can draw: every character `h2_euc_kr` knows, the rest dropped
/// (logged) — a dialog line is never worth refusing the dialog for.
fn euc_lossy(text: &str) -> Vec<u8> {
    if let Some(b) = crate::guest_build::h2_euc_kr(text) {
        return b;
    }
    let mut out = Vec::with_capacity(text.len() * 2);
    let mut dropped = 0;
    for c in text.chars() {
        match crate::guest_build::h2_euc_kr(c.encode_utf8(&mut [0u8; 4])) {
            Some(b) => out.extend_from_slice(&b),
            None => dropped += 1,
        }
    }
    log::warn!("save xfer: {dropped} character(s) the game's font lacks were left out of {text:?}");
    out
}

/// The file is staged (or may be): the app closes now — the next start applies it. Never returns.
fn quit_for_apply(how: crate::vault::Staged) -> ! {
    log::warn!("save xfer: staged ({how:?}) — closing the app; the next start applies it");
    crate::server_prompt::quit_app()
}

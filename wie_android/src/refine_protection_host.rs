#[derive(Default)]
struct RefineProtection {
    block: Option<u32>,
    refused: bool,
    dialog: Revive,
    consume: Option<(Arc<wie_backend::GuestCallResult>, RevivePrompt)>,
    resuming: Option<Arc<wie_backend::GuestCallResult>>,
    paid: bool,
}

impl RefineProtection {
    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused || crate::guest_build::hero2_java().is_none() { return; }
        let Some(block) = emulator.alloc_guest(crate::guest_build::RefineProtectionPatch::BLOCK_LEN) else { self.refused = true; return };
        let plan = crate::guest_build::h2_refine_protection_install(&|a, n| emulator.read_memory(a, n), block);
        let Some(writes) = plan else { self.refused = true; return };
        let mut done = 0;
        for (at, bytes) in &writes {
            if !emulator.write_memory(*at, bytes) { break; }
            done += 1;
        }
        if done == writes.len() && writes.iter().all(|(at, b)| emulator.read_memory(*at, b.len()) == *b) {
            self.block = Some(block);
            log::info!("refine protection: installed through {block:#x}, verified=true");
        } else {
            for (at, old, _) in crate::guest_build::h2_refine_protection_arms(block).into_iter().rev() { emulator.write_memory(at, &old); }
            self.refused = true;
            log::error!("refine protection: install failed, original refinement restored");
        }
    }

    fn state(&self, emulator: &dyn Emulator) -> Option<(u32, u32)> {
        crate::guest_build::h2_refine_protection_state(&|a, n| emulator.read_memory(a, n), self.block?)
    }

    fn busy(&self, emulator: &dyn Emulator) -> bool {
        self.dialog.holding() || self.consume.is_some() || self.resuming.is_some() || self.state(emulator).is_some_and(|(s, _)| s != 0)
    }

    fn continue_attempt(&mut self, emulator: &mut dyn Emulator, yes: bool) {
        let Some(block) = self.block else { return };
        if yes {
            let read = |a, n| emulator.read_memory(a, n);
            let selected = (|| {
                let (_, window) = self.state(emulator)?;
                let fields = crate::guest_build::h2_fields(&read, window)?;
                let item = crate::guest_build::h2_rd_u32(&read, fields + 0x30)?;
                let h2 = crate::guest_build::hero2_java()?;
                let bag = crate::guest_build::h2_static_handle(&read, h2.inventory_slot)?;
                Some((fields + 0x34, crate::guest_build::h2_bag_index_of(&read, bag, item)?))
            })();
            if let Some((at, index)) = selected { emulator.write_memory(at, &index.to_le_bytes()); }
        }
        if emulator.write_memory(block, &(if yes { 2u32 } else { 3u32 }).to_le_bytes()) {
            if let Some((_, window)) = self.state(emulator) {
                self.resuming = java_call(emulator, crate::guest_build::JavaCall::RefineAttempt, JavaThis::Handle(window), &[]);
            }
        }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, account: &mut AccountState, data_dir: &std::path::Path) {
        if let Some((out, prompt)) = self.consume.take() {
            match out.poll() {
                None => { self.consume = Some((out, prompt)); return; }
                Some(Ok(_)) => {
                    if account.bags.contains(&Some(prompt.orb.bag)) { account.dirty = true; }
                    self.paid = true;
                    self.continue_attempt(emulator, true);
                    log::info!("refine protection: warranty consumed, keeping previous refinement");
                }
                Some(Err(())) => {
                    self.dialog.prompt = Some(prompt);
                    self.dialog.send(proxy);
                    log::error!("refine protection: consume failed, asking again without destroying gear");
                    return;
                }
            }
        }
        if let Some(out) = self.resuming.take() {
            match out.poll() {
                None => { self.resuming = Some(out); return; }
                Some(Ok(_)) => {
                    self.paid = false;
                    let read = |a,n| emulator.read_memory(a,n);
                    let fields = self.state(emulator).and_then(|(_, w)| crate::guest_build::h2_fields(&read, w));
                    if let Some(fields) = fields {
                        emulator.write_memory(fields + 4 + crate::guest_build::Hero2Java::WIN_DIRTY_OFF, &1u32.to_le_bytes());
                    }
                }
                Some(Err(())) => { log::error!("refine protection: continuation failed"); return; }
            }
        }
        if self.dialog.prompt.as_ref().is_some_and(|p| p.sent.elapsed() >= REVIVE_RESEND) { self.dialog.send(proxy); }
        if self.dialog.holding() { return; }
        if !self.state(emulator).is_some_and(|(s, _)| s == 1) { return; }
        if self.paid { self.continue_attempt(emulator, true); return; }
        let Some(h2) = crate::guest_build::hero2_java() else { return };
        if !account.ready() && !account.construction_refused {
            account.ensure_bags(emulator, data_dir);
            return;
        }
        if account.pending.is_some() { return; }
        let read = |a: u32, n: usize| emulator.read_memory(a, n);
        let mut bags: Vec<u32> = crate::guest_build::h2_static_handle(&read, h2.inventory_slot).into_iter().collect();
        if account.ready() { bags.extend(account.bags.iter().enumerate().filter(|(i, _)| !account.load_failed[*i]).filter_map(|(_, b)| *b)); }
        let Some(orb) = crate::guest_build::h2_refine_warranty(&read, &bags) else { self.continue_attempt(emulator, false); return };
        if self.dialog.font.is_none() { self.dialog.font = crate::guest_build::h2_font_from_guest(&read); }
        let screen = (crate::guest_build::h2_rd_u32(&read, h2.screen.0), crate::guest_build::h2_rd_u32(&read, h2.screen.1), crate::guest_build::h2_rd_u32(&read, h2.screen.2));
        let (Some(sw), Some(sh), Some(cx)) = screen else { return };
        if self.dialog.font.is_none() { return; }
        self.dialog.prompt = Some(RevivePrompt {
            confirm: crate::guest_build::H2Confirm::new(), orb,
            lines: crate::guest_build::h2_refine_protection_lines(),
            screen: (sw as i32, sh as i32, cx as i32), sent: Instant::now(),
        });
        self.dialog.send(proxy);
        log::info!("refine protection: asking, warranty bag={:#x} record={:#x}", orb.bag, orb.record);
    }

    fn key(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, code: KeyCode, down: bool) -> bool {
        if !down {
            if let Some(i) = self.dialog.swallow_up.iter().position(|k| *k == code) { self.dialog.swallow_up.remove(i); return true; }
            return false;
        }
        if !self.busy(emulator) { return false; }
        if !self.dialog.swallow_up.contains(&code) { self.dialog.swallow_up.push(code); }
        use crate::guest_build::H2OptDir;
        let dir = match code {
            KeyCode::UP | KeyCode::NUM2 => Some(H2OptDir::Up),
            KeyCode::DOWN | KeyCode::NUM8 => Some(H2OptDir::Down),
            KeyCode::LEFT | KeyCode::NUM4 => Some(H2OptDir::Left),
            KeyCode::RIGHT | KeyCode::NUM6 => Some(H2OptDir::Right),
            KeyCode::OK | KeyCode::NUM5 => Some(H2OptDir::Ok),
            _ => None,
        };
        if let Some(p) = self.dialog.prompt.as_mut() { let act = p.confirm.key(dir); self.act(emulator, proxy, act); }
        true
    }

    fn tap(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, fx: u32, fy: u32) -> bool {
        if !self.busy(emulator) { return false; }
        if let Some(p) = self.dialog.prompt.as_mut() {
            let (sw, sh, cx) = p.screen;
            let hit = crate::guest_build::h2_confirm_hit(p.lines.len(), sw, sh, cx, fx as i32, fy as i32);
            let act = p.confirm.tap(hit);
            self.act(emulator, proxy, act);
        }
        true
    }

    fn act(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, act: crate::guest_build::H2ConfirmAct) {
        use crate::guest_build::H2ConfirmAct;
        match act {
            H2ConfirmAct::Moved => self.dialog.send(proxy),
            H2ConfirmAct::Swallowed => {},
            H2ConfirmAct::Chose(yes) => {
                let Some(p) = self.dialog.prompt.take() else { return };
                if !yes {
                    let _ = proxy.send_event(UserEvent::RevivePrompt(Vec::new()));
                    self.continue_attempt(emulator, false);
                    return;
                }
                let read = |a, n| emulator.read_memory(a, n);
                let valid = crate::guest_build::h2_refine_warranty(&read, &[p.orb.bag]);
                if let Some(orb) = valid.filter(|o| o.record == p.orb.record) {
                    if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::BagConsumeAt, JavaThis::Handle(orb.bag), &[wie_backend::JavaArg::Int(orb.index as i32), wie_backend::JavaArg::Int(1)]) {
                        self.consume = Some((out, p));
                        let _ = proxy.send_event(UserEvent::RevivePrompt(Vec::new()));
                        return;
                    }
                }
                self.dialog.prompt = Some(p);
                self.dialog.send(proxy);
            }
        }
    }
}

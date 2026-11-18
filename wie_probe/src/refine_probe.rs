#[derive(Default)]
struct RefineProbe {
    block: Option<u32>,
    window: Option<u32>,
    item: Option<u32>,
    warehouse: Option<u32>,
    new_record: Option<u32>,
    pending: Option<(Arc<GuestCallResult>, u8)>,
    config: Option<(u64, u32, bool, u8, bool)>,
    asked: bool,
    last: String,
}

impl RefineProbe {
    fn call(&mut self, emu: &mut dyn Emulator, this: JavaReceiver, method: &'static str, desc: &'static str, args: &[JavaArg], next: u8) {
        let out = Arc::new(GuestCallResult::new());
        emu.call_java_method(this, method, desc, args, out.clone());
        self.pending = Some((out, next));
    }

    fn poll(&mut self, emu: &mut dyn Emulator, f: u64, config: Option<(u64, u32, bool, u8, bool)>, out_dir: &Path) {
        type P = guest_build::RefineProtectionPatch;
        if f == 30 {
            let block = emu.alloc_guest(P::BLOCK_LEN).expect("refine block");
            let plan = guest_build::h2_refine_protection_install(&|a, n| emu.read_memory(a, n), block).expect("refine patch guard");
            for (at, bytes) in plan { assert!(emu.write_memory(at, &bytes)); }
            self.block = Some(block);
        }
        let Some(cfg @ (start, level, yes, roll, warehouse)) = config else { return };
        if f > start && (f - start) % 300 == 0 && self.pending.is_none() && self.item.is_some() {
            let rd = |a,n| emu.read_memory(a,n);
            let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
            let exists = guest_build::h2_bag(&rd, h2.inventory_slot).is_some_and(|b| b.items.iter().any(|i| Some(i.handle) == self.item));
            if exists { self.asked = false; self.prepare(emu, level, roll); }
        }
        if f == start {
            self.config = Some(cfg);
            self.call(emu, JavaReceiver::Class("ck"), "a", "()Lck;", &[], 0);
        }
        if let Some((out, step)) = self.pending.take() {
            match out.poll() {
                None => self.pending = Some((out, step)),
                Some(Err(())) => panic!("refine test step {step} failed"),
                Some(Ok(v)) => match step {
                    0 => {
                        assert_ne!(v, 0); self.window = Some(v);
                        self.call(emu, JavaReceiver::Handle { handle: v, class: "ck" }, "a", "()V", &[], 1);
                    }
                    1 => self.call(emu, JavaReceiver::Class("ag"), "h", "(I)V", &[JavaArg::Int(6)], 2),
                    2 if warehouse => { self.pending = Some((h2_call(emu, guest_build::JavaCall::BagNew, None, &[JavaArg::Int(127)]).unwrap(), 3)); },
                    2 => self.prepare(emu, level, roll),
                    3 => {
                        assert_ne!(v, 0); self.warehouse = Some(v);
                        self.call(emu, JavaReceiver::Class("ch"), "a", "(BBZZ)Lch;", &[JavaArg::Int(21), JavaArg::Int(6), JavaArg::Int(1), JavaArg::Int(0)], 4);
                    }
                    4 => { self.new_record = Some(v); self.call(emu, JavaReceiver::Handle { handle: self.warehouse.unwrap(), class: "bq" }, "a", "(Lch;B)I", &[JavaArg::Object(v), JavaArg::Int(2)], 5); },
                    5 => { if let Some(rec) = self.new_record.take() { emu.release_java_object(rec); } self.prepare(emu, level, roll); },
                    6 => { emu.write_memory(self.block.unwrap(), &2u32.to_le_bytes()); self.resume(emu); }
                    8 => {
                        let rd = |a,n| emu.read_memory(a,n);
                        let ib = guest_build::h2_fields(&rd, self.item.unwrap()).unwrap();
                        let cb = guest_build::h2_fields(&rd, self.window.unwrap()).unwrap();
                        emu.write_memory(ib + 0x34, &v.to_le_bytes());
                        emu.write_memory(cb + 0x44, &1u32.to_le_bytes());
                        emu.write_memory(cb + 4 + guest_build::Hero2Java::WIN_DIRTY_OFF, &1u32.to_le_bytes());
                    }
                    7 => {
                        if let Some(fields) = self.window.and_then(|w| guest_build::h2_fields(&|a,n| emu.read_memory(a,n), w)) {
                            emu.write_memory(fields + 4 + guest_build::Hero2Java::WIN_DIRTY_OFF, &1u32.to_le_bytes());
                        }
                    },
                    _ => unreachable!(),
                },
            }
        }
        let Some(block) = self.block else { return };
        let rd = |a, n| emu.read_memory(a, n);
        if guest_build::h2_refine_protection_state(&rd, block).is_some_and(|(s, _)| s == 1) && !self.asked {
            self.asked = true;
            let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
            let mut bags: Vec<u32> = guest_build::h2_static_handle(&rd, h2.inventory_slot).into_iter().collect();
            bags.extend(self.warehouse);
            let warranty = guest_build::h2_refine_warranty(&rd, &bags);
            eprintln!("[h2-refine] f={f} failed warranty={warranty:?} answer={yes}");
            if let Some(font) = guest_build::h2_font_from_guest(&rd) {
                let p = guest_build::h2_confirm_patch(&font, &guest_build::h2_refine_protection_lines(), true, 240, 320, 120);
                let mut ppm = format!("P6\n{} {}\n255\n", p.w, p.h).into_bytes();
                for c in p.argb { ppm.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]); }
                fs::create_dir_all(out_dir).unwrap(); fs::write(out_dir.join("warranty_prompt.ppm"), ppm).unwrap();
            }
            if let Some(w) = warranty.filter(|_| yes) {
                self.call(emu, JavaReceiver::Handle { handle: w.bag, class: "bq" }, "a", "(BI)V", &[JavaArg::Int(w.index as i32), JavaArg::Int(1)], 6);
            } else { emu.write_memory(block, &3u32.to_le_bytes()); self.resume(emu); }
        }
        if self.config.is_some() && f % 5 == 0 {
            let rd = |a, n| emu.read_memory(a, n);
            let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
            let inv = guest_build::h2_bag(&rd, h2.inventory_slot);
            let exists = inv.as_ref().is_some_and(|b| b.items.iter().any(|i| Some(i.handle) == self.item));
            let level = self.item.filter(|_| exists).and_then(|i| guest_build::h2_fields(&rd, i)).and_then(|b| guest_build::h2_rd_u32(&rd, b + 0x30));
            let warranty = inv.as_ref().map(|b| b.items.iter().filter(|i| i.kind == 21 && i.id == 6).map(|i| i.count).sum::<i32>());
            let wh = self.warehouse.and_then(|b| guest_build::h2_refine_warranty(&rd, &[b])).map_or(0, |i| i.total);
            let stones = inv.as_ref().map(|b| b.items.iter().filter(|i| i.kind == 20 && i.id == 0).map(|i| i.count).sum::<i32>());
            let gold = guest_build::h2_rd_u32(&rd, guest_build::ShopSellPatch::GOLD_SLOT);
            let line = format!("state={:?} item={exists} level={level:?} warranty={warranty:?} warehouse={wh} stones={stones:?} gold={gold:?}", guest_build::h2_refine_protection_state(&rd, block));
            if self.last != line { eprintln!("[h2-refine] f={f} {line}"); self.last = line; }
        }
    }

    fn resume(&mut self, emu: &mut dyn Emulator) {
        self.pending = Some((h2_call(emu, guest_build::JavaCall::RefineAttempt, self.window, &[]).unwrap(), 7));
    }

    fn prepare(&mut self, emu: &mut dyn Emulator, level: u32, roll: u8) {
        let rd = |a, n| emu.read_memory(a, n);
        let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
        let bag = guest_build::h2_bag(&rd, h2.inventory_slot).unwrap();
        let item = bag.items.iter().rev().find(|i| (0..=4).contains(&i.kind)).unwrap();
        self.item = Some(item.handle);
        let idx = guest_build::h2_bag_index_of(&rd, bag.handle, item.handle).unwrap();
        let ib = guest_build::h2_fields(&rd, item.handle).unwrap();
        let cb = guest_build::h2_fields(&rd, self.window.unwrap()).unwrap();
        let writes = [(ib + 0x30, level), (cb + 8, 0), (cb + 0x30, item.handle), (cb + 0x34, idx), (cb + 0x38, level), (cb + 0x3c, level + 1), (cb + 0x40, (level + 1) * 100), (cb + 0x44, 0)];
        for (a, v) in writes { assert!(emu.write_memory(a, &v.to_le_bytes())); }
        assert!(emu.write_memory(0x15d578, &[roll, 0x20, 0xc0, 0x46]));
        self.call(emu, JavaReceiver::Class("z"), "a", "(Lch;)B", &[JavaArg::Object(item.handle)], 8);
        eprintln!("[h2-refine] prepared item={:#x} index={idx} level={level} roll={roll}", item.handle);
    }
}

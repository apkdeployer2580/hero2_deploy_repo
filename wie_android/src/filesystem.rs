//! Persistent filesystem backed by synchronous `std::fs` under
//! `<app-private data dir>/<aid>/fs/<path>`. Ported verbatim from
//! `wie_cli/src/filesystem.rs`, replacing `directories::ProjectDirs` with the
//! Android app-private directory (`AndroidApp::internal_data_path`). Synchronous
//! disk writes are what make in-game save/continue robust on Android — a write
//! is on disk by the time the call returns.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use wie_backend::Filesystem;

pub struct AndroidFilesystem {
    base_path: PathBuf,
}

impl AndroidFilesystem {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn path_for(&self, aid: &str, path: &str) -> Option<PathBuf> {
        let sanitized_aid: String = aid.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        if sanitized_aid.is_empty() || sanitized_aid == "." || sanitized_aid == ".." {
            log::error!("rejected: invalid aid ({aid}, {path})");
            return None;
        }

        let mut normalized = PathBuf::new();
        for component in Path::new(path).components() {
            match component {
                Component::Normal(c) => normalized.push(c),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    log::error!("path traversal attempt rejected ({aid}, {path})");
                    return None;
                }
            }
        }

        if normalized.as_os_str().is_empty() {
            log::error!("rejected: empty normalized path ({aid}, {path})");
            return None;
        }

        // Every guest file operation passes through here, reads included, so this is the
        // earliest and most complete place to learn which archive root is live.
        crate::vault::note_app_root(&self.base_path, &sanitized_aid);
        Some(self.base_path.join(&sanitized_aid).join("fs").join(normalized))
    }
}

#[async_trait::async_trait]
impl Filesystem for AndroidFilesystem {
    async fn exists(&self, aid: &str, path: &str) -> bool {
        // ★ 2026-10-16 보스 레이드: while the gate is on, the temple is the gate-bearing copy (`raid_gate`) — served
        //   from here because the overlay reads the platform side first and the archive's own member second.
        if crate::raid_gate::serve(path).is_some() {
            return true;
        }
        let Some(disk_path) = self.path_for(aid, path) else {
            return false;
        };

        match disk_path.metadata() {
            Ok(md) => md.is_file(),
            Err(_) => false,
        }
    }

    async fn size(&self, aid: &str, path: &str) -> Option<usize> {
        if let Some(v) = crate::raid_gate::serve(path) {
            return Some(v.len());
        }
        let disk_path = self.path_for(aid, path)?;
        let md = disk_path.metadata().ok()?;
        if !md.is_file() {
            return None;
        }
        Some(md.len() as usize)
    }

    async fn read(&self, aid: &str, path: &str, offset: usize, count: usize, buf: &mut [u8]) -> Option<usize> {
        if let Some(v) = crate::raid_gate::serve(path) {
            if offset >= v.len() {
                return Some(0);
            }
            let n = core::cmp::min(count, v.len() - offset);
            buf[..n].copy_from_slice(&v[offset..offset + n]);
            return Some(n);
        }
        let disk_path = self.path_for(aid, path)?;
        // ★ 2026-10-16 보스 레이드: a save's `/g` is read before the map it stands in — the gate is decided from the record
        //   bound to it now, so the temple that follows in the same load already knows (`raid_gate::on_save_read`). Once
        //   per file (the first chunk), from the whole file; cheap for everything else (`classify` rejects on the first byte).
        if offset == 0
            && let Some((crate::savevault::Half::Prog, slot)) = crate::savevault::classify(path)
            && let Ok(g) = fs::read(&disk_path)
        {
            crate::raid_gate::on_save_read(&self.base_path, slot, &g);
        }

        let mut file = match OpenOptions::new().read(true).open(&disk_path) {
            Ok(f) => f,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return None;
                }
                log::warn!("read: open failed ({aid}, {path}): {err}");
                return None;
            }
        };

        let size = file.metadata().map(|m| m.len() as usize).unwrap_or(0);
        if offset >= size {
            return Some(0);
        }

        if let Err(err) = file.seek(SeekFrom::Start(offset as u64)) {
            log::warn!("read: seek failed ({aid}, {path}): {err}");
            return Some(0);
        }

        let to_read = core::cmp::min(count, size - offset);
        let slice = &mut buf[..to_read];
        match file.read_exact(slice) {
            Ok(()) => Some(to_read),
            Err(err) => {
                log::warn!("read: IO error ({aid}, {path}): {err}");
                Some(0)
            }
        }
    }

    async fn write(&self, aid: &str, path: &str, offset: usize, data: &[u8]) -> usize {
        let Some(disk_path) = self.path_for(aid, path) else {
            return 0;
        };

        // ★ BEFORE the file stops being what it was. Together with `truncate`
        // below these are the only two ways a guest file can change — the
        // `Filesystem` trait has no delete — so this pair is the whole surface
        // the save vault has to watch. Cheap for everything that is not a save:
        // `savevault::classify` rejects on the first character.
        crate::vault::before_overwrite(&self.base_path, aid, path);

        if let Some(parent) = disk_path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!("write: create parent dir failed ({aid}, {path}): {err}");
            return 0;
        }

        let mut file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&disk_path) {
            Ok(f) => f,
            Err(err) => {
                log::warn!("write: open failed ({aid}, {path}): {err}");
                return 0;
            }
        };

        let current_size = file.metadata().map(|m| m.len() as usize).unwrap_or(0);

        if offset > current_size {
            if let Err(err) = file.set_len(offset as u64) {
                log::warn!("write: set_len extend failed ({aid}, {path}): {err}");
                return 0;
            }
        }

        if let Err(err) = file.seek(SeekFrom::Start(offset as u64)) {
            log::warn!("write: seek failed ({aid}, {path}): {err}");
            return 0;
        }

        match file.write_all(data) {
            Ok(()) => data.len(),
            Err(err) => {
                log::warn!("write: write_all failed ({aid}, {path}): {err}");
                0
            }
        }
    }

    async fn truncate(&self, aid: &str, path: &str, len: usize) {
        let Some(disk_path) = self.path_for(aid, path) else {
            return;
        };

        // ★ The edge that actually destroys a save. `org.kwis.msp.io.File` opened
        // with `Mode::WRITE_TRUNC` calls `setLength(0)` before the guest writes a
        // byte (`wie_wipi_java/.../io/file.rs`), so by the time `write` runs the
        // old contents are already gone.
        crate::vault::before_overwrite(&self.base_path, aid, path);

        if let Some(parent) = disk_path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!("truncate: create parent dir failed ({aid}, {path}): {err}");
            return;
        }

        let file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&disk_path) {
            Ok(f) => f,
            Err(err) => {
                log::warn!("truncate: open failed ({aid}, {path}): {err}");
                return;
            }
        };

        if let Err(err) = file.set_len(len as u64) {
            log::warn!("truncate: set_len failed ({aid}, {path}): {err}");
        }
    }
}

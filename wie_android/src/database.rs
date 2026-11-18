//! Record-store database backed by synchronous `std::fs` under
//! `<app-private data dir>/<app_id>/db/<name>/<record_id>`. Ported from
//! `wie_cli/src/database.rs`, replacing `directories::ProjectDirs` with the
//! Android app-private directory. Hero3/Hero4 saves live mostly here (WIPI DB
//! channel), so the synchronous `fs::write` matters for save-then-kill safety.

use std::{fs, path::PathBuf};

use wie_backend::RecordId;

pub struct AndroidDatabaseRepository {
    base_path: PathBuf,
}

impl AndroidDatabaseRepository {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn get_path_for_database(&self, name: &str, app_id: &str) -> PathBuf {
        let sanitized_app_id: String = app_id.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        let app_id = if sanitized_app_id.is_empty() || sanitized_app_id == "." || sanitized_app_id == ".." {
            "_"
        } else {
            &sanitized_app_id
        };

        let name: String = name.chars().map(|c| if matches!(c, '\\' | '\0') { '_' } else { c }).collect();
        let mut normalized_name = PathBuf::new();
        for segment in name.trim_start_matches('/').split('/') {
            match segment {
                "" | "." => {}
                ".." => normalized_name.push("_"),
                segment => normalized_name.push(segment),
            }
        }
        if normalized_name.as_os_str().is_empty() {
            normalized_name.push("_");
        }

        self.base_path.join(app_id).join("db").join(normalized_name)
    }
}

#[async_trait::async_trait]
impl wie_backend::DatabaseRepository for AndroidDatabaseRepository {
    async fn open(&self, name: &str, app_id: &str) -> Box<dyn wie_backend::Database> {
        let path = self.get_path_for_database(name, app_id);
        log::info!("DB open name={name:?} app_id={app_id:?}");
        Box::new(Database::new(path).unwrap())
    }

    async fn exists(&self, name: &str, app_id: &str) -> bool {
        let r = self.get_path_for_database(name, app_id).exists();
        log::info!("DB exists name={name:?} app_id={app_id:?} -> {r}");
        r
    }

    async fn delete(&self, name: &str, app_id: &str) -> bool {
        let path = self.get_path_for_database(name, app_id);
        match fs::remove_dir_all(path) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                log::warn!("Failed to delete database: {e}");
                false
            }
        }
    }
}

pub struct Database {
    base_path: PathBuf,
}

impl Database {
    pub fn new(base_path: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&base_path)?;
        Ok(Self { base_path })
    }

    fn find_empty_record_id(&self) -> RecordId {
        let mut record_id = 1; // XXX midp requires first record to be 1
        loop {
            let path = self.base_path.join(record_id.to_string());
            if !path.exists() {
                return record_id;
            }
            record_id += 1;
        }
    }

    fn get_path_for_record(&self, id: RecordId) -> PathBuf {
        self.base_path.join(id.to_string())
    }
}

#[async_trait::async_trait]
impl wie_backend::Database for Database {
    async fn next_id(&self) -> RecordId {
        let id = self.find_empty_record_id();
        log::info!("DB next_id [{:?}] -> {id}", self.base_path.file_name());
        id
    }

    async fn add(&mut self, data: &[u8]) -> RecordId {
        let id = self.find_empty_record_id();
        let path = self.get_path_for_record(id);
        let ok = fs::write(&path, data).is_ok();
        log::info!("DB add [{:?}] id={id} len={} ok={ok}", self.base_path.file_name(), data.len());
        id
    }

    async fn get(&self, id: RecordId) -> Option<Vec<u8>> {
        let path = self.get_path_for_record(id);
        let r = fs::read(path).ok();
        log::info!("DB get [{:?}] id={id} -> {:?} bytes", self.base_path.file_name(), r.as_ref().map(|v| v.len()));
        r
    }

    async fn set(&mut self, id: RecordId, data: &[u8]) -> bool {
        let path = self.get_path_for_record(id);
        let ok = fs::write(path, data).is_ok();
        log::info!("DB set [{:?}] id={id} len={} ok={ok}", self.base_path.file_name(), data.len());
        ok
    }

    async fn delete(&mut self, id: RecordId) -> bool {
        let path = self.get_path_for_record(id);
        let ok = fs::remove_file(path).is_ok();
        log::info!("DB delete [{:?}] id={id} ok={ok}", self.base_path.file_name());
        ok
    }

    async fn get_record_ids(&self) -> Vec<RecordId> {
        let ids: Vec<RecordId> = match fs::read_dir(&self.base_path) {
            Ok(rd) => rd
                .filter_map(|x| x.ok())
                .filter(|x| x.path().is_file())
                .filter_map(|x| x.file_name().to_str().and_then(|s| s.parse().ok()))
                .collect(),
            Err(_) => Vec::new(),
        };
        log::info!("DB get_record_ids [{:?}] -> {ids:?}", self.base_path.file_name());
        ids
    }
}

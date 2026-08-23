use std::path::PathBuf;
use std::{env, fs};

use anyhow::Context;
use chrono::{DateTime, Duration, Local};
use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::session::Session;
use crate::task::Task;

#[derive(Serialize, Deserialize)]
pub struct StoredAgenda {
    pub tasks: Vec<Task>,
    pub log: Vec<Session>,
    pub pin: Option<Block>,
    pub planned_at: DateTime<Local>,
    pub clock_offset: Duration,
}

impl StoredAgenda {
    const DIRECTORY: &str = "lateen";
    const FILE: &str = "state.json";
    const STAGED: &str = "state.json.new";
    const RESET: &str = "LATEEN_RESET_STATE";
    const EMPTY: &str = "LATEEN_EMPTY_STATE";

    pub fn load() -> Option<Self> {
        let path = Self::directory()?.join(Self::FILE);

        if env::var_os(Self::RESET).is_some() || Self::starts_empty() {
            let _ = fs::remove_file(&path);

            return None;
        }

        serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()
    }

    pub fn starts_empty() -> bool {
        env::var_os(Self::EMPTY).is_some()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let directory = Self::directory().context("no state directory")?;
        let staged = directory.join(Self::STAGED);

        fs::create_dir_all(&directory)?;
        fs::write(&staged, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&staged, directory.join(Self::FILE))?;

        Ok(())
    }

    fn directory() -> Option<PathBuf> {
        let state = match env::var_os("XDG_STATE_HOME").map(PathBuf::from) {
            Some(state) if state.is_absolute() => state,
            _ => PathBuf::from(env::var_os("HOME")?).join(".local/state"),
        };

        Some(state.join(Self::DIRECTORY))
    }
}

use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

#[derive(Serialize, Deserialize, Clone)]
pub struct Instance {
    pub name: String,
    pub version: String,
    pub ram_mb: u32,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub username: String,
    pub java_path: String,
    pub ms_client_id: String,
    pub instances: Vec<Instance>,
    pub selected: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            username: "Player".into(),
            java_path: "java".into(),
            ms_client_id: String::new(),
            instances: vec![Instance {
                name: "Vanilla".into(),
                version: "1.21.1".into(),
                ram_mb: 4096,
            }],
            selected: 0,
        }
    }
}

pub fn data_root() -> PathBuf {
    directories::ProjectDirs::from("dev", "horizon", "HorizonNext")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("horizon_data"))
}

impl Config {
    pub fn load(root: &Path) -> Self {
        fs::read_to_string(root.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) {
        let _ = fs::create_dir_all(root);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(root.join("config.json"), s);
        }
    }
}

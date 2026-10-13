use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_PATH: &str = if cfg!(debug_assertions) {
    "example-config.toml"
} else {
    "/etc/beegit.toml"
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub scan: ScanConfig,
    pub server: ServerConfig,
    pub repo: HashMap<String, RepoConfig>,
}

impl Config {
    pub(crate) fn load() -> Self {
        match Self::load_from_file(CONFIG_PATH) {
            Ok(config) => config,
            Err(e) => {
                eprintln!(
                    "\x1b[33mWarning: could not read config at {CONFIG_PATH} ({e}), using defaults\x1b[0m"
                );
                Self::default()
            }
        }
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let absolute_path = std::fs::canonicalize(path)?;
        let content = fs::read_to_string(absolute_path)?;

        let mut config: Config = toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("\x1b[31m{e}\x1b[0m");
            std::process::exit(1);
        });

        config.scan.scan_path = fs::canonicalize(&config.scan.scan_path)
            .unwrap_or_else(|e| {
                eprintln!("\x1b[31mInvalid scan_path: {e}\x1b[0m");
                std::process::exit(1);
            })
            .to_string_lossy()
            .into_owned();

        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub scan_path: String,
    pub scan_hidden_path: bool,
    pub strip_repo_suffix: bool,
    pub scan_none_bare_repos: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            scan_path: String::from("/var/lib/git/"),
            scan_hidden_path: false,
            strip_repo_suffix: true,
            scan_none_bare_repos: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: [u8; 4],
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: [127, 0, 0, 1],
            port: 8000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RepoConfig {
    pub name: Option<String>,
    pub path: Option<PathBuf>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub owner: Option<String>,
    pub default_branch: Option<String>,
}

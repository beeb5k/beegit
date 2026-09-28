use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Path to the configuration file.
///
/// During development, the configuration is loaded from the project's
/// `example-config.toml`. In release builds, BeeGit uses the system-wide
/// configuration at `/etc/beegit.toml`.
pub const CONFIG_PATH: &str = if cfg!(debug_assertions) {
    "example-config.toml"
} else {
    "/etc/beegit.toml"
};

/// Root configuration for BeeGit.
///
/// The configuration is divided into two sections:
///
/// - [`ScanConfig`] controls how BeeGit discovers and names repositories.
/// - [`ServerConfig`] controls the address and port of the HTTP server.
///
/// Missing fields are filled with their corresponding [`Default`] values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Repository scanning configuration.
    pub scan: ScanConfig,

    /// HTTP server configuration.
    pub server: ServerConfig,
}

impl Config {
    /// Loads the BeeGit configuration from [`CONFIG_PATH`].
    ///
    /// If the configuration file cannot be read or resolved, the default
    /// configuration is returned.
    ///
    /// Parsing errors are considered fatal and terminate the process after
    /// printing the error to stderr.
    pub fn load() -> Self {
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

    /// Loads and parses a configuration from the specified file.
    ///
    /// The configuration path is canonicalized before the file is read.
    /// The repository scan path is also canonicalized after deserialization
    /// so that the rest of the application receives an absolute path.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
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

/// Configuration controlling how BeeGit discovers Git repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    /// Root directory from which BeeGit begins searching for repositories.
    ///
    /// The path is canonicalized when the configuration is loaded, so the
    /// application receives an absolute path.
    pub scan_path: String,

    /// Whether hidden directories are included during repository discovery.
    ///
    /// When disabled, directories whose names begin with `.` are skipped.
    pub scan_hidden_path: bool,

    /// Whether the `.git` suffix is removed from repository names.
    ///
    /// For example, a repository directory named `project.git` is displayed
    /// as `project` when this option is enabled.
    pub strip_repo_suffix: bool,

    /// Whether non-bare Git repositories are included in the scan.
    ///
    /// When disabled, BeeGit only considers bare repositories.
    pub scan_none_bare_repos: bool,
}

impl Default for ScanConfig {
    /// Returns the default repository scanning configuration.
    fn default() -> Self {
        Self {
            scan_path: String::from("/var/lib/git/"),
            scan_hidden_path: false,
            strip_repo_suffix: true,
            scan_none_bare_repos: false,
        }
    }
}

/// Configuration for the BeeGit HTTP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// IPv4 address on which the HTTP server listens.
    ///
    /// The default address, `127.0.0.1`, restricts connections to the local
    /// machine. Use `0.0.0.0` if BeeGit should accept connections on all
    /// available IPv4 interfaces.
    pub host: [u8; 4],

    /// TCP port on which the HTTP server listens.
    pub port: u16,
}

impl Default for ServerConfig {
    /// Returns the default HTTP server configuration.
    fn default() -> Self {
        Self {
            host: [127, 0, 0, 1],
            port: 8000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let path = std::env::temp_dir().join(format!(
            "beegit_config_{}_{}",
            std::process::id(),
            timestamp
        ));

        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn loads_config_from_file() {
        let dir = temp_dir();
        let config_path = dir.join("beegit.toml");
        let repos = dir.join("repos");

        fs::create_dir_all(&repos).unwrap();

        fs::write(
            &config_path,
            format!(
                r#"
                [scan]
                scan_path = "{}"

                [server]
                port = 3000
            "#,
                repos.display()
            ),
        )
        .unwrap();

        let config = Config::load_from_file(&config_path).unwrap();

        assert_eq!(config.server.port, 3000);
        assert_eq!(
            PathBuf::from(config.scan.scan_path),
            fs::canonicalize(&repos).unwrap()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_fields_use_defaults() {
        let dir = temp_dir();
        let config_path = dir.join("beegit.toml");
        let repos = dir.join("repos");

        fs::create_dir_all(&repos).unwrap();

        fs::write(
            &config_path,
            format!(
                r#"
                [scan]
                scan_path = "{}"
            "#,
                repos.display()
            ),
        )
        .unwrap();

        let config = Config::load_from_file(&config_path).unwrap();

        assert_eq!(
            config.scan.scan_path,
            fs::canonicalize(&repos).unwrap().to_string_lossy()
        );

        assert!(!config.scan.scan_hidden_path);
        assert!(config.scan.strip_repo_suffix);
        assert!(!config.scan.scan_none_bare_repos);

        assert_eq!(config.server.host, [127, 0, 0, 1]);
        assert_eq!(config.server.port, 8000);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_config_returns_error() {
        let dir = temp_dir();
        let config_path = dir.join("does-not-exist.toml");

        let result = Config::load_from_file(&config_path);

        assert!(result.is_err());

        fs::remove_dir_all(dir).unwrap();
    }
}

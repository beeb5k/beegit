use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, RepoConfig},
    git::{RepoKind, detect_git_repo},
};

pub fn scan_tree() -> Vec<RepoConfig> {
    let mut cfg = Config::load();
    let root = PathBuf::from(&cfg.scan.scan_path);

    if !matches!(detect_git_repo(&root, &cfg.scan), RepoKind::Nope) {
        eprintln!("\x1b[31mError: scan path cannot be a Git repository.\x1b[0m");
        std::process::exit(1);
    }

    let mut repos = Vec::new();

    scan_path(&root, &mut cfg, &mut repos);

    repos
}

fn scan_path(dir_path: &Path, cfg: &mut Config, repos: &mut Vec<RepoConfig>) {
    let repo_kind = detect_git_repo(dir_path, &cfg.scan);

    let is_repo = match repo_kind {
        RepoKind::Bare => true,
        RepoKind::NonBare => cfg.scan.scan_none_bare_repos,
        RepoKind::Nope => false,
    };

    if is_repo {
        if let Some(repo) = add_git_repo(dir_path, repo_kind, cfg) {
            repos.push(repo);
        }

        return;
    }

    let entries = match std::fs::read_dir(dir_path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.as_encoded_bytes();

        // Don't scan the `.git` directory in non-bare repositories.
        if name == b".git" {
            continue;
        }

        if name.starts_with(b".") && !cfg.scan.scan_hidden_path {
            continue;
        }

        let entry_path = entry.path();

        if entry_path.is_dir() && !entry_path.is_symlink() {
            scan_path(&entry_path, cfg, repos);
        }
    }
}

fn add_git_repo(path: &Path, repo_kind: RepoKind, cfg: &mut Config) -> Option<RepoConfig> {
    let git_dir = match repo_kind {
        RepoKind::NonBare => path.join(".git/"),
        _ => path.to_path_buf(),
    };

    let repo_name = path.file_name()?.to_string_lossy();

    let repo_name = match cfg.scan.strip_repo_suffix {
        true => repo_name.strip_suffix(".git").unwrap_or(&repo_name),
        false => &repo_name,
    };

    let mut repo_config = cfg
        .repo
        .remove(repo_name)
        .or_else(|| cfg.repo.remove(&format!("{repo_name}.git")))
        .unwrap_or_default();

    if repo_config.name.is_none() {
        repo_config.name = Some(repo_name.to_string());
    }

    if repo_config.description.is_none() {
        let desc_path = git_dir.join("description");

        let description = std::fs::read_to_string(desc_path)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "Unnamed repository".into());

        repo_config.description = Some(description);
    }

    if repo_config.path.is_none() {
        repo_config.path = Some(path.to_path_buf());
    }

    if repo_config.category.is_none() {
        let cat = path
            .strip_prefix(&cfg.scan.scan_path)
            .ok()?
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|p| !p.is_empty());
        repo_config.category = cat;
    }

    Some(repo_config)
}

fn _parse_git_config(path: &Path, repo_kind: RepoKind) -> Option<HashMap<String, String>> {
    let config_path = match repo_kind {
        RepoKind::Bare => path.join("config"),
        RepoKind::NonBare => path.join(".git/config"),
        RepoKind::Nope => return None,
    };

    let config = std::fs::read_to_string(config_path).ok()?;

    let mut values = HashMap::new();

    for line in config.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        values.insert(key.trim().to_owned(), value.trim().to_owned());
    }

    Some(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            let path = std::env::temp_dir().join(format!(
                "beegit_scanner_{}_{}",
                std::process::id(),
                timestamp
            ));

            fs::create_dir_all(&path).unwrap();

            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn config(root: &Path) -> Config {
        let mut cfg = Config::default();
        cfg.scan.scan_path = root.to_string_lossy().into_owned();
        cfg
    }

    fn bare_repo(path: &Path, description: &str) {
        fs::create_dir_all(path.join("objects")).unwrap();

        fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(path.join("config"), "[core]\n\tbare = true\n").unwrap();
        fs::write(path.join("description"), description).unwrap();
    }

    fn non_bare_repo(path: &Path, description: &str) {
        let git_dir = path.join(".git");

        fs::create_dir_all(git_dir.join("objects")).unwrap();

        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(git_dir.join("config"), "[core]\n\tbare = false\n").unwrap();
        fs::write(git_dir.join("description"), description).unwrap();
    }

    fn scan(root: &Path, mut cfg: Config) -> Vec<RepoConfig> {
        let mut repos = Vec::new();
        scan_path(root, &mut cfg, &mut repos);
        repos
    }

    #[test]
    fn detects_bare_repository() {
        let root = TempDir::new();
        bare_repo(root.path(), "Test repository");

        assert!(matches!(
            detect_git_repo(root.path(), &config(root.path()).scan),
            RepoKind::Bare
        ));
    }

    #[test]
    fn detects_non_bare_repository() {
        let root = TempDir::new();
        non_bare_repo(root.path(), "Test repository");

        let mut cfg = config(root.path());
        cfg.scan.scan_none_bare_repos = true;

        assert!(matches!(
            detect_git_repo(root.path(), &cfg.scan),
            RepoKind::NonBare
        ));
    }

    #[test]
    fn detects_non_repository() {
        let root = TempDir::new();

        assert!(matches!(
            detect_git_repo(root.path(), &config(root.path()).scan),
            RepoKind::Nope
        ));
    }

    #[test]
    fn detects_non_bare_repository_only_when_enabled() {
        let root = TempDir::new();
        non_bare_repo(root.path(), "Test repository");

        let mut cfg = config(root.path());

        assert!(matches!(
            detect_git_repo(root.path(), &cfg.scan),
            RepoKind::Nope
        ));

        cfg.scan.scan_none_bare_repos = true;

        assert!(matches!(
            detect_git_repo(root.path(), &cfg.scan),
            RepoKind::NonBare
        ));
    }

    #[test]
    fn scans_bare_repository() {
        let root = TempDir::new();
        let repo_path = root.path().join("foo.git");

        bare_repo(&repo_path, "Foo repository");

        let repos = scan(root.path(), config(root.path()));

        assert_eq!(repos.len(), 1);

        let repo = &repos[0];

        assert_eq!(repo.name.as_deref(), Some("foo"));
        assert_eq!(repo.description.as_deref(), Some("Foo repository"));
        assert_eq!(repo.path.as_deref(), Some(repo_path.as_path()));
        assert_eq!(repo.category, None);
    }

    #[test]
    fn scans_non_bare_repository_when_enabled() {
        let root = TempDir::new();
        let repo_path = root.path().join("foo");

        non_bare_repo(&repo_path, "Foo repository");

        let mut cfg = config(root.path());
        cfg.scan.scan_none_bare_repos = true;

        let repos = scan(root.path(), cfg);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some("foo"));
        assert_eq!(repos[0].description.as_deref(), Some("Foo repository"));
    }

    #[test]
    fn skips_non_bare_repository_when_disabled() {
        let root = TempDir::new();
        let repo_path = root.path().join("foo");

        non_bare_repo(&repo_path, "Foo repository");

        let repos = scan(root.path(), config(root.path()));

        assert!(repos.is_empty());
    }

    #[test]
    fn scans_nested_repository_and_sets_category() {
        let root = TempDir::new();
        let repo_path = root.path().join("misc").join("shit").join("dark_shit.git");

        fs::create_dir_all(repo_path.parent().unwrap()).unwrap();
        bare_repo(&repo_path, "Dark repository");

        let repos = scan(root.path(), config(root.path()));

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some("dark_shit"));
        assert_eq!(repos[0].category.as_deref(), Some("misc/shit"));
    }

    #[test]
    fn preserves_nested_category() {
        let root = TempDir::new();
        let repo_path = root
            .path()
            .join("misc")
            .join("shit")
            .join("dark_shit")
            .join("goon_count.git");

        fs::create_dir_all(repo_path.parent().unwrap()).unwrap();
        bare_repo(&repo_path, "Goon count");

        let repos = scan(root.path(), config(root.path()));

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some("goon_count"));
        assert_eq!(repos[0].category.as_deref(), Some("misc/shit/dark_shit"));
    }

    #[test]
    fn skips_git_directory() {
        let root = TempDir::new();
        let repo_path = root.path().join("project");

        non_bare_repo(&repo_path, "Project");
        bare_repo(&repo_path.join(".git").join("fake.git"), "Fake repository");

        let mut cfg = config(root.path());
        cfg.scan.scan_none_bare_repos = true;

        let repos = scan(root.path(), cfg);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some("project"));
    }

    #[test]
    fn skips_hidden_directories_by_default() {
        let root = TempDir::new();

        bare_repo(&root.path().join("visible.git"), "Visible");
        bare_repo(&root.path().join(".hidden.git"), "Hidden");

        let repos = scan(root.path(), config(root.path()));

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some("visible"));
    }

    #[test]
    fn scans_hidden_directories_when_enabled() {
        let root = TempDir::new();

        bare_repo(&root.path().join(".hidden.git"), "Hidden");

        let mut cfg = config(root.path());
        cfg.scan.scan_hidden_path = true;

        let repos = scan(root.path(), cfg);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some(".hidden"));
    }

    #[test]
    fn keeps_git_suffix_when_disabled() {
        let root = TempDir::new();

        bare_repo(&root.path().join("foo.git"), "Foo");

        let mut cfg = config(root.path());
        cfg.scan.strip_repo_suffix = false;

        let repos = scan(root.path(), cfg);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name.as_deref(), Some("foo.git"));
    }

    #[test]
    fn preserves_repository_configuration() {
        let root = TempDir::new();
        let repo_path = root.path().join("foo.git");

        bare_repo(&repo_path, "Git description");

        let mut cfg = config(root.path());

        cfg.repo.insert(
            "foo".into(),
            RepoConfig {
                name: Some("Custom Foo".into()),
                path: None,
                description: Some("Configured description".into()),
                category: Some("random".into()),
                owner: Some("beeb5k".into()),
                default_branch: Some("main".into()),
            },
        );

        let repos = scan(root.path(), cfg);

        assert_eq!(repos.len(), 1);

        let repo = &repos[0];

        assert_eq!(repo.name.as_deref(), Some("Custom Foo"));
        assert_eq!(repo.description.as_deref(), Some("Configured description"));
        assert_eq!(repo.category.as_deref(), Some("random"));
        assert_eq!(repo.owner.as_deref(), Some("beeb5k"));
        assert_eq!(repo.default_branch.as_deref(), Some("main"));
        assert_eq!(repo.path.as_deref(), Some(repo_path.as_path()));
    }

    #[test]
    fn fills_missing_repository_configuration() {
        let root = TempDir::new();
        let repo_path = root.path().join("foo.git");

        bare_repo(&repo_path, "Git description");

        let mut cfg = config(root.path());

        cfg.repo.insert(
            "foo".into(),
            RepoConfig {
                name: None,
                path: None,
                description: None,
                category: None,
                owner: Some("beeb5k".into()),
                default_branch: Some("main".into()),
            },
        );

        let repos = scan(root.path(), cfg);

        assert_eq!(repos.len(), 1);

        let repo = &repos[0];

        assert_eq!(repo.name.as_deref(), Some("foo"));
        assert_eq!(repo.description.as_deref(), Some("Git description"));
        assert_eq!(repo.category, None);
        assert_eq!(repo.owner.as_deref(), Some("beeb5k"));
        assert_eq!(repo.default_branch.as_deref(), Some("main"));
        assert_eq!(repo.path.as_deref(), Some(repo_path.as_path()));
    }

    #[test]
    fn uses_unnamed_repository_when_description_is_missing() {
        let root = TempDir::new();
        let repo_path = root.path().join("foo.git");

        fs::create_dir_all(repo_path.join("objects")).unwrap();
        fs::write(repo_path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(repo_path.join("config"), "[core]\n\tbare = true\n").unwrap();

        let repos = scan(root.path(), config(root.path()));

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].description.as_deref(), Some("Unnamed repository"));
    }

    #[test]
    fn parses_git_config() {
        let root = TempDir::new();

        fs::write(
            root.path().join("config"),
            "\
gitweb.owner = beeb5k
gitweb.description = fast git frontend
gitweb.category = random
",
        )
        .unwrap();

        let values = _parse_git_config(root.path(), RepoKind::Bare).unwrap();

        assert_eq!(values.get("gitweb.owner"), Some(&"beeb5k".to_string()));
        assert_eq!(
            values.get("gitweb.description"),
            Some(&"fast git frontend".to_string())
        );
        assert_eq!(values.get("gitweb.category"), Some(&"random".to_string()));
    }
}

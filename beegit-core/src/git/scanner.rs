use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, RepoConfig, ScanConfig},
    git::repo::Repo,
};

pub(crate) enum RepoKind {
    Nope,
    Bare,
    NonBare,
}

/// Returns the type of Git repository found at a path.
pub(crate) fn detect_git_repo(path: &Path, cfg: &ScanConfig) -> RepoKind {
    let check = |p: &Path| {
        if !(p.join("HEAD").is_file() && p.join("objects").is_dir()) {
            return RepoKind::Nope;
        }

        let is_bare = match std::fs::read_to_string(p.join("config")) {
            Ok(config) => config.contains("bare = true"),
            Err(_) => false,
        };

        if is_bare {
            RepoKind::Bare
        } else {
            RepoKind::NonBare
        }
    };

    let result = check(path);

    if !matches!(result, RepoKind::Nope) {
        return result;
    }

    if cfg.scan_none_bare_repos {
        return check(&path.join(".git"));
    }

    RepoKind::Nope
}

/// Recursively scans a directory for Git repositories.
///
/// The scan path itself must not be a Git repository. Discovered repositories
/// are added to the returned list, and repository directories are not
/// traversed further.
///
/// Bare repositories are always included. Non-bare repositories are included
/// only when [`ScanConfig::scan_none_bare_repos`] is enabled.
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

/// Scans a directory recursively and adds discovered repositories to `repos`.
///
/// Repository directories are not traversed after they are discovered.
/// `.git` directories are skipped because they are handled as part of their
/// containing non-bare repository. Hidden paths are skipped unless enabled
/// through [`ScanConfig::scan_hidden_path`].
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

        // A non-bare repository stores its Git metadata in `.git`, so it must
        // not be scanned as a separate directory.
        if name == b".git" {
            continue;
        }

        // Hidden directories are skipped unless explicitly enabled.
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

        repo_config.name = Some(description);
    }

    if repo_config.path.is_none() {
        repo_config.path = Some(path.to_path_buf());
    }

    if repo_config.category.is_none() {
        repo_config.category = Some(path.to_path_buf());
    }

    todo!()
}

fn parse_git_config(path: &Path, repo_kind: RepoKind) -> Option<HashMap<String, String>> {
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
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
        path
    }

    fn create_bare_repo(path: &Path) {
        fs::create_dir_all(path.join("objects")).unwrap();

        fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        fs::write(
            path.join("config"),
            "[core]\n\
             repositoryformatversion = 0\n\
             bare = true\n",
        )
        .unwrap();
    }

    fn create_non_bare_repo(path: &Path) {
        let git = path.join(".git");

        fs::create_dir_all(git.join("objects")).unwrap();

        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        fs::write(
            git.join("config"),
            "[core]\n\
             repositoryformatversion = 0\n\
             bare = false\n",
        )
        .unwrap();
    }

    fn config(path: &Path) -> ScanConfig {
        ScanConfig {
            scan_path: path.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    fn is_scan_path_repo(path: &Path) -> bool {
        is_bare_git_repo(path) || is_non_bare_repo(path)
    }

    #[test]
    fn identifies_bare_repository() {
        let dir = temp_dir();
        create_bare_repo(&dir);

        assert!(is_git_repo(&dir));
        assert!(is_bare_git_repo(&dir));
        assert!(!is_non_bare_repo(&dir));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn identifies_non_bare_repository() {
        let dir = temp_dir();
        create_non_bare_repo(&dir);

        assert!(is_non_bare_repo(&dir));
        assert!(!is_bare_git_repo(&dir));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ignores_invalid_repository() {
        let dir = temp_dir();

        assert!(!is_git_repo(&dir));
        assert!(!is_bare_git_repo(&dir));
        assert!(!is_non_bare_repo(&dir));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scans_bare_repositories() {
        let dir = temp_dir();

        create_bare_repo(&dir.join("repo1.git"));
        create_bare_repo(&dir.join("repo2.git"));

        let repos = scan(&config(&dir));

        assert_eq!(repos.len(), 2);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scans_repositories_recursively() {
        let dir = temp_dir();

        create_bare_repo(&dir.join("projects/repo1.git"));
        create_bare_repo(&dir.join("projects/other/repo2.git"));

        let repos = scan(&config(&dir));

        assert_eq!(repos.len(), 2);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ignores_non_bare_repositories_by_default() {
        let dir = temp_dir();

        create_non_bare_repo(&dir.join("repo"));

        let repos = scan(&config(&dir));

        assert!(repos.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scans_non_bare_repositories_when_enabled() {
        let dir = temp_dir();

        create_non_bare_repo(&dir.join("repo"));

        let mut scan_config = config(&dir);
        scan_config.scan_none_bare_repos = true;

        let repos = scan(&scan_config);

        assert_eq!(repos.len(), 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ignores_hidden_directories_by_default() {
        let dir = temp_dir();

        create_bare_repo(&dir.join(".hidden/repo.git"));

        let repos = scan(&config(&dir));

        assert!(repos.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scans_hidden_directories_when_enabled() {
        let dir = temp_dir();

        create_bare_repo(&dir.join(".hidden/repo.git"));

        let mut scan_config = config(&dir);
        scan_config.scan_hidden_path = true;

        let repos = scan(&scan_config);

        assert_eq!(repos.len(), 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn skips_git_directory() {
        let dir = temp_dir();

        create_non_bare_repo(&dir.join("project"));

        // The `.git` directory contains Git metadata but must not be
        // discovered as a separate bare repository.
        let repos = scan(&config(&dir));

        assert!(repos.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_scan_path_that_is_a_git_repository() {
        let dir = temp_dir();
        create_bare_repo(&dir);

        assert!(is_scan_path_repo(&dir));

        fs::remove_dir_all(dir).unwrap();
    }
}

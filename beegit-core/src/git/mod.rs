use std::{collections::HashMap, path::Path};

use crate::{config::ScanConfig, git::repo::Repo};

pub mod repo;
pub mod scanner;

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
pub fn scan_tree(scan_cfg: &ScanConfig) -> Vec<Repo> {
    let root = Path::new(&scan_cfg.scan_path);

    if !matches!(detect_git_repo(root, scan_cfg), RepoKind::Nope) {
        eprintln!("\x1b[31mError: scan path cannot be a Git repository.\x1b[0m");
        std::process::exit(1);
    }

    let mut repos = Vec::new();

    scan_path(root, scan_cfg, &mut repos);

    repos
}

/// Scans a directory recursively and adds discovered repositories to `repos`.
///
/// Repository directories are not traversed after they are discovered.
/// `.git` directories are skipped because they are handled as part of their
/// containing non-bare repository. Hidden paths are skipped unless enabled
/// through [`ScanConfig::scan_hidden_path`].
fn scan_path(dir_path: &Path, scan_cfg: &ScanConfig, repos: &mut Vec<Repo>) {
    let repo_kind = detect_git_repo(dir_path, scan_cfg);

    let is_repo = match repo_kind {
        RepoKind::Bare => true,
        RepoKind::NonBare => scan_cfg.scan_none_bare_repos,
        RepoKind::Nope => false,
    };

    if is_repo {
        if let Some(repo) = add_git_repo(dir_path, repo_kind, scan_cfg) {
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
        if name.starts_with(b".") && !scan_cfg.scan_hidden_path {
            continue;
        }

        let entry_path = entry.path();

        if entry_path.is_dir() && !entry_path.is_symlink() {
            scan_path(&entry_path, scan_cfg, repos);
        }
    }
}

fn add_git_repo(path: &Path, repo_kind: RepoKind, scan_cfg: &ScanConfig) -> Option<Repo> {
    let repo_name = path.file_name()?.to_string_lossy();

    let repo_name = match scan_cfg.strip_repo_suffix {
        true => repo_name.strip_suffix(".git").unwrap_or(&repo_name),
        false => &repo_name,
    };

    if scan_cfg.enable_git_config {
        let git_config = parse_git_config(path, repo_kind);

        let get_config_val = |k: &str| git_config.as_ref().and_then(|config| config.get(k));

        let owner = get_config_val("gitweb.owner");
        let desc = get_config_val("gitweb.description");
        let catogery = get_config_val("gitweb.catogery");
        Repo {
            path: path.to_path_buf(),
            name: repo_name.to_string(),
            owner: owner.unwrap().to_string(),
            description: desc.unwrap().to_string(),
        };
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

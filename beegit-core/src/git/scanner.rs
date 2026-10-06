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

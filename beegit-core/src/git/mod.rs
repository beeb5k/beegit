// pub mod repo;
pub mod scanner;
use std::path::Path;

use crate::config::ScanConfig;

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

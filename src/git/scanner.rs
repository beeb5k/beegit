use crate::config::ScanConfig;
use crate::git::repo::Repo;
use std::path::Path;

/// Recursively scans a directory for Git repositories.
///
/// The scan starts at [`ScanConfig::scan_path`] and walks through its
/// subdirectories. Bare repositories are always included, while non-bare
/// repositories can be included with [`ScanConfig::scan_none_bare_repos`].
/// Hidden directories are skipped unless [`ScanConfig::scan_hidden_path`]
/// is enabled.
///
/// The scan stops descending into a directory once it has been identified
/// as a repository.
pub fn scan(scan: &ScanConfig) -> Vec<Repo> {
    let root = Path::new(&scan.scan_path);

    // Exit if the scan path is a repository.
    if is_bare_git_repo(root) || is_non_bare_repo(root) {
        eprintln!("\x1b[31mError: scan path cannot be a Git repository.\x1b[0m");
        std::process::exit(1);
    }

    let mut repos = Vec::new();

    scan_dir(root, scan, &mut repos);

    repos
}

/// Recursively walks a directory and adds discovered repositories to `repos`.
///
/// Repository directories are not traversed further. `.git` directories and
/// hidden paths are skipped according to the scan configuration.
fn scan_dir(dir_path: &Path, scan: &ScanConfig, repos: &mut Vec<Repo>) {
    let is_repo =
        is_bare_git_repo(dir_path) || (scan.scan_none_bare_repos && is_non_bare_repo(dir_path));

    if is_repo {
        if let Some(repo) =
            Repo::read_metadata_from_path(dir_path.to_path_buf(), scan.strip_repo_suffix)
        {
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

        // A non-bare repository stores its Git data in `.git`, so it must
        // not be treated as a separate directory during the scan.
        if name == b".git" {
            continue;
        }

        // Hidden directories are only scanned when explicitly enabled.
        if name.starts_with(b".") && !scan.scan_hidden_path {
            continue;
        }

        let entry_path = entry.path();

        if entry_path.is_dir() {
            scan_dir(&entry_path, scan, repos);
        }
    }
}

/// Checks whether a path contains the basic structure of a Git repository.
///
/// This only verifies the presence of `HEAD` and the `objects` directory.
/// The repository type is determined separately.
fn is_git_repo(path: &Path) -> bool {
    path.join("HEAD").is_file() && path.join("objects").is_dir()
}

/// Checks whether a path contains a non-bare Git repository.
///
/// Non-bare repositories keep their Git data in a `.git` directory. The
/// repository configuration is checked to distinguish them from ordinary
/// directories containing Git-related files.
fn is_non_bare_repo(path: &Path) -> bool {
    let git_dir = path.join(".git");

    if !is_git_repo(&git_dir) {
        return false;
    }

    match std::fs::read_to_string(git_dir.join("config")) {
        Ok(config) => config.contains("bare = false"),
        Err(_) => false,
    }
}

/// Checks whether a path is a bare Git repository.
///
/// Bare repositories keep their Git data directly in the repository root
/// instead of using a `.git` directory.
fn is_bare_git_repo(path: &Path) -> bool {
    if !is_git_repo(path) {
        return false;
    }

    match std::fs::read_to_string(path.join("config")) {
        Ok(config) => config.contains("bare = true"),
        Err(_) => false,
    }
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

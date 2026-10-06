use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use gix::utils::AsBStr;

/// Information about a Git repository.
#[derive(Debug, Clone)]
pub struct Repo {
    /// Display name of the repository.
    pub name: String,

    /// Filesystem path to the repository.
    pub path: PathBuf,

    /// Repository description read from Git's `description` file.
    pub description: String,
}

impl Repo {
    /// Builds repository metadata from its filesystem path.
    ///
    /// The repository name is derived from the final path component. When
    /// `strip_repo_suffix` is enabled, a trailing `.git` suffix is removed.
    ///
    /// The description is read from `description` for bare repositories or
    /// `.git/description` for non-bare repositories. If the description
    /// cannot be read, `Unnamed repository` is used.
    pub fn read_metadata_from_path(path: PathBuf, strip_repo_suffix: bool) -> Option<Self> {
        let mut name = path.file_name()?.to_string_lossy().to_string();

        if strip_repo_suffix {
            name = name.strip_suffix(".git").unwrap_or(&name).to_string();
        }

        let desc_path = match path.join("description").is_file() {
            true => path.join("description"),
            false => path.join(".git/description"),
        };

        let description = fs::read_to_string(desc_path)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "Unnamed repository".into());

        Some(Self {
            name,
            path,
            description,
        })
    }

    /// Returns the time elapsed since the commit currently pointed to by `HEAD`.
    ///
    /// The duration is calculated from the latest commit's timestamp to the
    /// current time and formatted as seconds, minutes, hours, days, weeks,
    /// months, or years.
    pub fn idle(&self) -> Option<String> {
        let commit_time = self.last_commit_time?.seconds + self.last_commit_time?.offset as i64;

        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;

        let seconds = now.saturating_sub(commit_time) as u64;

        Some(Self::format_duration(seconds))
    }

    /// Formats a duration in seconds using a human-readable time unit.
    fn format_duration(seconds: u64) -> String {
        let unit = |value: u64, singular: &str, plural: &str| match value {
            1 => format!("1 {singular}"),
            _ => format!("{value} {plural}"),
        };

        match seconds {
            0..=59 => unit(seconds, "second", "seconds"),
            60..=3_599 => unit(seconds / 60, "minute", "minutes"),
            3_600..=86_399 => unit(seconds / 3_600, "hour", "hours"),
            86_400..=604_799 => unit(seconds / 86_400, "day", "days"),
            604_800..=2_629_743 => unit(seconds / 604_800, "week", "weeks"),
            2_629_744..=31_556_927 => unit(seconds / 2_629_744, "month", "months"),
            _ => unit(seconds / 31_556_928, "year", "years"),
        }
    }

    /// Returns the committer timestamp of the commit currently pointed to by `HEAD`.
    pub fn last_commit_time(path: &Path) -> Option<gix::date::Time> {
        let repo = gix::ThreadSafeRepository::open(path).ok()?;
        let repo = repo.to_thread_local();
        let commit = repo.head().ok()?.peel_to_commit().ok()?;

        commit.time().ok()
    }

    pub fn read_readme(path: &Path) -> Option<String> {
        let repo = gix::ThreadSafeRepository::open(path).ok()?;
        let repo = repo.to_thread_local();
        let tree = repo.head().ok()?.peel_to_commit().ok()?.tree().ok()?;

        let candidates = [
            "README.md",
            "README",
            "README.txt",
            "README.markdown",
            "README.rst",
        ];

        for name in candidates {
            let Some(entry) = tree.find_entry(name) else {
                continue;
            };

            let object = entry.object().ok()?;
            let bytes = object.data.as_bstr();

            return Some(bytes.to_string());
        }

        None
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

        let path =
            std::env::temp_dir().join(format!("beegit_repo_{}_{}", std::process::id(), timestamp));

        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn creates_repo_read_metadata_from_path() {
        let dir = temp_dir();

        fs::write(dir.join("description"), "My repository\n").unwrap();

        let repo = Repo::read_metadata_from_path(dir.clone(), true).unwrap();

        assert_eq!(repo.name, dir.file_name().unwrap().to_string_lossy());
        assert_eq!(repo.path, dir);
        assert_eq!(repo.description, "My repository");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn strips_git_suffix() {
        let dir = temp_dir().join("project.git");
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("description"), "My repository").unwrap();

        let repo = Repo::read_metadata_from_path(dir.clone(), true).unwrap();

        assert_eq!(repo.name, "project");

        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn keeps_git_suffix_when_disabled() {
        let dir = temp_dir().join("project.git");
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("description"), "My repository").unwrap();

        let repo = Repo::read_metadata_from_path(dir.clone(), false).unwrap();

        assert_eq!(repo.name, "project.git");

        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn uses_default_description_when_missing() {
        let dir = temp_dir();

        let repo = Repo::read_metadata_from_path(dir.clone(), true).unwrap();

        assert_eq!(repo.description, "Unnamed repository");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn formats_duration() {
        assert_eq!(Repo::format_duration(0), "0 seconds");
        assert_eq!(Repo::format_duration(1), "1 second");
        assert_eq!(Repo::format_duration(60), "1 minute");
        assert_eq!(Repo::format_duration(3_600), "1 hour");
        assert_eq!(Repo::format_duration(86_400), "1 day");
        assert_eq!(Repo::format_duration(604_800), "1 week");
        assert_eq!(Repo::format_duration(2_629_744), "1 month");
        assert_eq!(Repo::format_duration(31_556_928), "1 year");
    }
}

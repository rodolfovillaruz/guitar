use std::ffi::OsStr;
use std::path::Path;

use git2::{Repository, RepositoryOpenFlags};

// The environment variables git uses to point at a repository that does not live at the
// conventional `<worktree>/.git` location.
const GIT_LOCATION_VARS: [&str; 2] = ["GIT_DIR", "GIT_WORK_TREE"];

// True when the environment points git at a specific repository or work tree.
pub fn git_env_active() -> bool {
    GIT_LOCATION_VARS.iter().any(|name| std::env::var_os(name).is_some())
}

// Open the repository at `path`, deferring to git's GIT_DIR / GIT_WORK_TREE when they are set.
//
// Once either variable is present libgit2's `FROM_ENV` path also honours the related
// GIT_COMMON_DIR, GIT_INDEX_FILE, GIT_OBJECT_DIRECTORY, GIT_ALTERNATE_OBJECT_DIRECTORIES and
// GIT_NAMESPACE overrides, matching how the `git` binary behaves in the same environment.
pub fn open_repo(path: impl AsRef<Path>) -> Result<Repository, git2::Error> {
    // With GIT_DIR set, git uses it as the repository directly and ignores discovery.
    if std::env::var_os("GIT_DIR").is_some() {
        return Repository::open_from_env();
    }

    // GIT_WORK_TREE (and friends) without GIT_DIR: discover the git directory starting
    // from `path`, then let FROM_ENV apply the work-tree and related overrides.
    if git_env_active() {
        return Repository::open_ext(path, RepositoryOpenFlags::FROM_ENV, std::iter::empty::<&OsStr>());
    }

    Repository::open(path)
}

#[cfg(test)]
#[path = "../../tests/git/os/repo.rs"]
mod tests;

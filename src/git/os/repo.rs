use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
// `path` always wins when it resolves to a repository: every caller passes the repository
// guitar is already focused on (its root, git dir, or work tree), and a GIT_DIR inherited
// from the surrounding shell must not silently redirect those opens somewhere else. GIT_DIR
// is only consulted as a fallback, when `path` is not a repository on its own.
//
// When GIT_WORK_TREE (and friends) are set without GIT_DIR, discovery still starts from
// `path`; libgit2's `FROM_ENV` path just layers GIT_WORK_TREE, GIT_COMMON_DIR,
// GIT_INDEX_FILE, GIT_OBJECT_DIRECTORY, GIT_ALTERNATE_OBJECT_DIRECTORIES and GIT_NAMESPACE
// on top, matching how the `git` binary behaves in the same environment.
pub fn open_repo(path: impl AsRef<Path>) -> Result<Repository, git2::Error> {
    let path = path.as_ref();

    // GIT_WORK_TREE (and friends) without GIT_DIR: FROM_ENV already discovers from `path`.
    if git_env_active() && std::env::var_os("GIT_DIR").is_none() {
        return Repository::open_ext(path, RepositoryOpenFlags::FROM_ENV, std::iter::empty::<&OsStr>());
    }

    // Prefer the repository at `path`; fall back to GIT_DIR only when there is none.
    match Repository::open(path) {
        Ok(repo) => Ok(attach_env_work_tree(repo)),
        Err(direct_err) => {
            if std::env::var_os("GIT_DIR").is_some() {
                // GIT_DIR names the git directory outright; libgit2 skips discovery for it.
                Ok(attach_env_work_tree(Repository::open_from_env()?))
            } else {
                Err(direct_err)
            }
        },
    }
}

// libgit2's `open_from_env` drops GIT_WORK_TREE when the git directory's config says
// `core.bare = true`: `load_workdir` returns early on a bare repository before it ever
// consults the environment. The `git` binary instead lets an explicit work tree win over
// `core.bare` for that invocation, so mirror it by re-attaching the work tree ourselves.
//
// Only applied when GIT_DIR actually selected this repository, so a GIT_WORK_TREE meant for
// one repository is never grafted onto another that `open_repo` happened to open by path.
// `set_workdir` is passed `update_gitlink = false` so the user's on-disk config is never
// touched. A relative GIT_WORK_TREE is resolved against the current directory, matching git.
fn attach_env_work_tree(repo: Repository) -> Repository {
    if repo.is_bare()
        && env_selected_repo(&repo)
        && let Some(work_tree) = std::env::var_os("GIT_WORK_TREE")
    {
        // A failure here leaves the repository bare; callers already tolerate that.
        let _ = repo.set_workdir(Path::new(&work_tree), false);
    }

    repo
}

// True when GIT_DIR is set and points at this repository's git directory.
fn env_selected_repo(repo: &Repository) -> bool {
    let Some(git_dir) = std::env::var_os("GIT_DIR") else {
        return false;
    };
    let want = std::fs::canonicalize(&git_dir).unwrap_or_else(|_| PathBuf::from(git_dir));
    let have = std::fs::canonicalize(repo.path()).unwrap_or_else(|_| repo.path().to_path_buf());
    want == have
}

#[cfg(test)]
#[path = "../../tests/git/os/repo.rs"]
mod tests;

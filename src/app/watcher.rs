use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};

use walkdir::WalkDir;

// Single files whose metadata tracks a discrete repository state change. Paths are
// relative to a git directory.
const WATCHED_FILES: &[&str] = &["HEAD", "index", "packed-refs", "ORIG_HEAD", "MERGE_HEAD", "FETCH_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "REBASE_HEAD", "BISECT_LOG", "COMMIT_EDITMSG"];

// Ref and reflog trees, walked recursively. These stay small even on huge
// repositories; the object database is deliberately never walked.
const WATCHED_TREES: &[&str] = &["refs", "logs", "worktrees"];

const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(3);

// The UI loop ticks roughly every 50ms; hashing the ref trees that often is
// wasteful, so the watcher only re-scans on this cadence.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Debounced poller for a repository's git directory.
///
/// The main loop calls [`GitWatcher::poll`] once per tick. It hashes the
/// modification metadata of the files and ref trees that reflect meaningful
/// repository state (HEAD moves, staging, fetches, branch and tag updates,
/// in-progress operations). When that hash changes a debounce timer starts;
/// once the git directory has been quiet for `debounce`, `poll` returns `true`
/// so the caller can reload exactly as if the user pressed the reload key.
pub struct GitWatcher {
    dirs: Vec<PathBuf>,
    debounce: Duration,
    poll_interval: Duration,
    last_poll: Option<Instant>,
    signature: u64,
    pending_since: Option<Instant>,
}

impl GitWatcher {
    /// `git_dir` is the repository's git directory (`Repository::path`), and
    /// `common_dir` the shared directory that holds `refs`/`packed-refs` for
    /// linked worktrees (`Repository::commondir`). They are usually the same
    /// path, in which case it is only scanned once.
    pub fn new(git_dir: PathBuf, common_dir: PathBuf) -> Self {
        let mut dirs = vec![git_dir];
        if !dirs.contains(&common_dir) {
            dirs.push(common_dir);
        }

        let mut watcher = Self { dirs, debounce: DEFAULT_DEBOUNCE, poll_interval: DEFAULT_POLL_INTERVAL, last_poll: None, signature: 0, pending_since: None };
        // Seed the baseline so the first real change is what schedules a reload,
        // not the watcher coming online.
        watcher.signature = watcher.compute_signature();
        watcher
    }

    /// Re-scan the git directory (at most once per poll interval) and report
    /// whether a debounced reload is now due. Returns `true` at most once per
    /// burst of activity, after `debounce` of quiet.
    pub fn poll(&mut self, now: Instant) -> bool {
        let due_for_scan = match self.last_poll {
            Some(last) => now.duration_since(last) >= self.poll_interval,
            None => true,
        };

        if due_for_scan {
            self.last_poll = Some(now);
            let signature = self.compute_signature();
            if signature != self.signature {
                self.signature = signature;
                self.pending_since = Some(now);
            }
        }

        match self.pending_since {
            Some(since) if now.duration_since(since) >= self.debounce => {
                self.pending_since = None;
                true
            },
            _ => false,
        }
    }

    fn compute_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for dir in &self.dirs {
            for file in WATCHED_FILES {
                hash_path(&mut hasher, &dir.join(file));
            }
            for tree in WATCHED_TREES {
                hash_tree(&mut hasher, &dir.join(tree));
            }
        }
        hasher.finish()
    }
}

// Fold a single path's identity and modification metadata into the hash. Absent
// paths still contribute so that their creation or removal registers as a change.
fn hash_path(hasher: &mut DefaultHasher, path: &Path) {
    path.to_string_lossy().hash(hasher);
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            meta.len().hash(hasher);
            if let Ok(modified) = meta.modified()
                && let Ok(since_epoch) = modified.duration_since(UNIX_EPOCH)
            {
                since_epoch.as_nanos().hash(hasher);
            }
        },
        Err(_) => {
            u8::MAX.hash(hasher);
        },
    }
}

fn hash_tree(hasher: &mut DefaultHasher, root: &Path) {
    if !root.exists() {
        hash_path(hasher, root);
        return;
    }
    for entry in WalkDir::new(root).sort_by_file_name().into_iter().flatten() {
        hash_path(hasher, entry.path());
    }
}

#[cfg(test)]
#[path = "../tests/app/watcher.rs"]
mod tests;

use super::*;
use crate::git::actions::worktrees::create_worktree;
use git2::{Repository, Signature};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = env::temp_dir().join(format!("guitar-{name}-{}-{suffix}", process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn init_repo(path: &Path) -> Repository {
    let repo = Repository::init_opts(path, git2::RepositoryInitOptions::new().initial_head("master")).unwrap();
    fs::write(path.join("file.txt"), "hello\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("file.txt")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = Signature::now("Tester", "tester@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
    drop(tree);
    repo
}

#[test]
fn lists_main_and_linked_worktrees() {
    let dir = TestDir::new("worktree-list");
    let repo_path = dir.path.join("repo");
    let worktree_path = dir.path.join("repo-feature");
    fs::create_dir_all(&repo_path).unwrap();
    let repo = init_repo(&repo_path);
    let oid = repo.head().unwrap().target().unwrap();

    create_worktree(&repo, "feature", &worktree_path, oid).unwrap();

    let entries = list_worktrees(&repo, Some(&repo_path)).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| entry.is_main() && entry.is_current));
    assert!(entries.iter().any(|entry| entry.name == "feature" && entry.is_linked() && entry.is_valid));
}

#[test]
fn marks_current_linked_worktree() {
    let dir = TestDir::new("worktree-current");
    let repo_path = dir.path.join("repo");
    let worktree_path = dir.path.join("repo-feature");
    fs::create_dir_all(&repo_path).unwrap();
    let repo = init_repo(&repo_path);
    let oid = repo.head().unwrap().target().unwrap();

    create_worktree(&repo, "feature", &worktree_path, oid).unwrap();
    let linked_repo = Repository::open(&worktree_path).unwrap();

    let entries = list_worktrees(&linked_repo, Some(&worktree_path)).unwrap();
    assert!(entries.iter().any(|entry| entry.name == "feature" && entry.is_current));
    assert!(entries.iter().any(|entry| entry.is_main() && !entry.is_current));
}

use super::*;
use git2::{Repository, ResetType, Signature};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_repo(name: &str) -> (PathBuf, Repository) {
    let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("guitar-reflog-query-{name}-{id}"));
    fs::create_dir_all(&path).unwrap();
    let repo = Repository::init_opts(&path, git2::RepositoryInitOptions::new().initial_head("master")).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    (path, repo)
}

fn commit(repo: &Repository, file: &str, message: &str) -> Oid {
    let workdir = repo.workdir().unwrap().to_path_buf();
    fs::write(workdir.join(file), message).unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new(file)).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents).unwrap()
}

#[test]
fn head_reflog_keeps_commit_after_reset() {
    let (_path, repo) = temp_repo("lost-head");
    let base = commit(&repo, "file.txt", "base");
    let lost = commit(&repo, "file.txt", "lost");
    let base_commit = repo.find_commit(base).unwrap();
    repo.reset(base_commit.as_object(), ResetType::Hard, None).unwrap();

    let entries = get_head_reflog_entries(&repo).unwrap();

    assert!(entries.iter().any(|entry| entry.new_oid == lost && entry.selector.starts_with("HEAD@{")));
    assert_eq!(repo.head().unwrap().target(), Some(base));
}

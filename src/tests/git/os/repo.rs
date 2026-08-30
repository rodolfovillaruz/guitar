use super::{git_env_active, open_repo};
use git2::{Repository, Signature};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

// Serialises the tests in this file: they mutate process-wide git environment variables.
static ENV_GUARD: Mutex<()> = Mutex::new(());

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("guitar-open-repo-{name}-{}-{suffix}", process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// Restores each variable to whatever it was before the test on drop.
struct ScopedEnv {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl ScopedEnv {
    fn set(vars: &[(&'static str, &Path)]) -> Self {
        let saved = vars.iter().map(|(name, _)| (*name, std::env::var_os(name))).collect();
        for (name, value) in vars {
            // SAFETY: tests in this file hold ENV_GUARD, so no other thread reads the environment concurrently.
            unsafe { std::env::set_var(name, value) };
        }
        Self { saved }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            // SAFETY: see ScopedEnv::set.
            match value {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

fn init_repo_with_commit(work: &Path) -> Repository {
    let repo = Repository::init(work).unwrap();
    fs::write(work.join("file.txt"), "contents").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("file.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = Signature::now("Tester", "tester@example.com").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[]).unwrap();
    drop(tree);
    repo
}

#[test]
fn open_repo_without_env_opens_the_given_path() {
    let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = TestDir::new("plain");
    let work = dir.path.join("work");
    fs::create_dir_all(&work).unwrap();
    init_repo_with_commit(&work);

    let opened = open_repo(&work).unwrap();
    assert_eq!(canonical(opened.workdir().unwrap()), canonical(&work));
}

#[test]
fn open_repo_honors_git_dir_and_work_tree_env() {
    let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = TestDir::new("env");
    let work = dir.path.join("work");
    fs::create_dir_all(&work).unwrap();
    init_repo_with_commit(&work);

    // A directory that is not a repository: opening it directly must fail.
    let elsewhere = dir.path.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    assert!(Repository::open(&elsewhere).is_err());

    let git_dir = work.join(".git");
    let _env = ScopedEnv::set(&[("GIT_DIR", git_dir.as_path()), ("GIT_WORK_TREE", work.as_path())]);
    assert!(git_env_active());

    let opened = open_repo(&elsewhere).unwrap();
    assert_eq!(canonical(opened.path()), canonical(&git_dir));
    assert_eq!(canonical(opened.workdir().unwrap()), canonical(&work));
}

#[test]
fn open_repo_honors_work_tree_env_without_git_dir() {
    let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = TestDir::new("worktree-only");
    let work = dir.path.join("work");
    let nested = work.join("src/app");
    fs::create_dir_all(&nested).unwrap();
    init_repo_with_commit(&work);

    let relocated_work = dir.path.join("relocated");
    fs::create_dir_all(&relocated_work).unwrap();

    // Only GIT_WORK_TREE is set; GIT_DIR is discovered from the start path.
    let _env = ScopedEnv::set(&[("GIT_WORK_TREE", relocated_work.as_path())]);
    let opened = open_repo(&nested).unwrap();
    assert_eq!(canonical(opened.path()), canonical(&work.join(".git")));
    assert_eq!(canonical(opened.workdir().unwrap()), canonical(&relocated_work));
}

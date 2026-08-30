use super::*;
use std::{
    env, fs,
    path::PathBuf,
    process,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = env::temp_dir().join(format!("guitar-watcher-{name}-{}-{suffix}", process::id()));
        fs::create_dir_all(path.join("refs/heads")).unwrap();
        fs::create_dir_all(path.join("logs")).unwrap();
        fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(path.join("refs/heads/main"), "0000000000000000000000000000000000000000\n").unwrap();
        Self { path }
    }

    fn write(&self, rel: &str, contents: &str) {
        let target = self.path.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(target, contents).unwrap();
    }

    fn watcher(&self) -> GitWatcher {
        GitWatcher::new(self.path.clone(), self.path.clone())
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const DEBOUNCE: Duration = Duration::from_secs(3);

#[test]
fn quiet_directory_never_asks_for_reload() {
    let dir = TestDir::new("quiet");
    let mut watcher = dir.watcher();
    let start = Instant::now();

    for seconds in 0..20 {
        assert!(!watcher.poll(start + Duration::from_secs(seconds)));
    }
}

#[test]
fn change_triggers_reload_once_after_debounce() {
    let dir = TestDir::new("change");
    let mut watcher = dir.watcher();
    let start = Instant::now();

    // Baseline scan, nothing pending yet.
    assert!(!watcher.poll(start));

    dir.write("refs/heads/main", "1111111111111111111111111111111111111111\n");

    // Detected, but the debounce has not elapsed.
    assert!(!watcher.poll(start + Duration::from_millis(600)));
    assert!(!watcher.poll(start + Duration::from_secs(2)));

    // Debounce elapsed: exactly one reload is requested.
    assert!(watcher.poll(start + Duration::from_millis(600) + DEBOUNCE));
    assert!(!watcher.poll(start + Duration::from_secs(30)));
}

#[test]
fn continued_activity_keeps_resetting_the_debounce() {
    let dir = TestDir::new("burst");
    let mut watcher = dir.watcher();
    let start = Instant::now();
    assert!(!watcher.poll(start));

    // A write every second for five seconds: the debounce never gets to expire.
    for seconds in 1..=5 {
        dir.write("refs/heads/main", &format!("{seconds:040}\n"));
        assert!(!watcher.poll(start + Duration::from_secs(seconds)));
    }

    // Still within the debounce window after the last change.
    assert!(!watcher.poll(start + Duration::from_secs(5) + Duration::from_secs(2)));
    // Quiet for the full debounce: now it fires.
    assert!(watcher.poll(start + Duration::from_secs(5) + DEBOUNCE));
}

#[test]
fn new_and_removed_paths_are_detected() {
    let dir = TestDir::new("create-remove");
    let mut watcher = dir.watcher();
    let start = Instant::now();
    assert!(!watcher.poll(start));

    fs::write(dir.path.join("ORIG_HEAD"), "2222222222222222222222222222222222222222\n").unwrap();
    assert!(!watcher.poll(start + Duration::from_secs(1)));
    assert!(watcher.poll(start + Duration::from_secs(1) + DEBOUNCE));

    fs::remove_file(dir.path.join("ORIG_HEAD")).unwrap();
    assert!(!watcher.poll(start + Duration::from_secs(10)));
    assert!(watcher.poll(start + Duration::from_secs(10) + DEBOUNCE));
}

#[test]
fn scans_are_throttled_between_polls() {
    let dir = TestDir::new("throttle");
    let mut watcher = dir.watcher();
    let start = Instant::now();
    assert!(!watcher.poll(start));

    dir.write("refs/heads/main", "3333333333333333333333333333333333333333\n");

    // A poll inside the poll interval does not re-scan, so the change is not seen yet
    // and no debounce is started.
    assert!(!watcher.poll(start + Duration::from_millis(100)));
    // Long after the debounce would have elapsed had the change been noticed: still
    // nothing, because the first scan that observes the change only happens now.
    assert!(!watcher.poll(start + Duration::from_secs(1)));
    assert!(watcher.poll(start + Duration::from_secs(1) + DEBOUNCE));
}

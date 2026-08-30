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
fn first_change_triggers_reload_immediately() {
    let dir = TestDir::new("change");
    let mut watcher = dir.watcher();
    let start = Instant::now();

    // Baseline scan, nothing pending yet.
    assert!(!watcher.poll(start));

    dir.write("refs/heads/main", "1111111111111111111111111111111111111111\n");

    // The first observed change fires on the leading edge, without waiting.
    assert!(watcher.poll(start + Duration::from_millis(600)));

    // A lone change owes no trailing reload; the quiet window stays silent.
    assert!(!watcher.poll(start + Duration::from_secs(2)));
    assert!(!watcher.poll(start + Duration::from_millis(600) + DEBOUNCE));
    assert!(!watcher.poll(start + Duration::from_secs(30)));
}

#[test]
fn burst_fires_once_up_front_and_once_after_it_settles() {
    let dir = TestDir::new("burst");
    let mut watcher = dir.watcher();
    let start = Instant::now();
    assert!(!watcher.poll(start));

    // First write of the burst reloads straight away.
    dir.write("refs/heads/main", &format!("{:040}\n", 1));
    assert!(watcher.poll(start + Duration::from_secs(1)));

    // More writes every second: collapsed, each one sliding the quiet window.
    for seconds in 2..=5 {
        dir.write("refs/heads/main", &format!("{seconds:040}\n"));
        assert!(!watcher.poll(start + Duration::from_secs(seconds)));
    }

    // Still within the quiet window after the last change.
    assert!(!watcher.poll(start + Duration::from_secs(5) + Duration::from_secs(2)));
    // Quiet for the full debounce: one trailing reload for the settled state.
    assert!(watcher.poll(start + Duration::from_secs(5) + DEBOUNCE));
    // ...and only one.
    assert!(!watcher.poll(start + Duration::from_secs(60)));
}

#[test]
fn new_and_removed_paths_are_detected() {
    let dir = TestDir::new("create-remove");
    let mut watcher = dir.watcher();
    let start = Instant::now();
    assert!(!watcher.poll(start));

    fs::write(dir.path.join("ORIG_HEAD"), "2222222222222222222222222222222222222222\n").unwrap();
    assert!(watcher.poll(start + Duration::from_secs(1)));

    // Quiet window closes with nothing owed.
    assert!(!watcher.poll(start + Duration::from_secs(1) + DEBOUNCE));

    fs::remove_file(dir.path.join("ORIG_HEAD")).unwrap();
    assert!(watcher.poll(start + Duration::from_secs(10)));
}

#[test]
fn scans_are_throttled_between_polls() {
    let dir = TestDir::new("throttle");
    let mut watcher = dir.watcher();
    let start = Instant::now();
    assert!(!watcher.poll(start));

    dir.write("refs/heads/main", "3333333333333333333333333333333333333333\n");

    // A poll inside the poll interval does not re-scan, so the change is not seen yet.
    assert!(!watcher.poll(start + Duration::from_millis(100)));
    // The first scan that observes the change fires immediately.
    assert!(watcher.poll(start + Duration::from_secs(1)));
}

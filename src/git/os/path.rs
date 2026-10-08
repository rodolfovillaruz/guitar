use std::path::{Path, PathBuf};

// Walk upward from any path until a non-bare repository root is found.
pub fn try_into_git_repo_root(start_path: impl AsRef<Path>) -> Option<PathBuf> {
    let mut current_path = start_path.as_ref();
    if current_path.is_file() {
        current_path = current_path.parent()?;
    }

    loop {
        let git_path = current_path.join(".git");
        if git_path.exists() {
            return Some(current_path.to_path_buf());
        }

        current_path = current_path.parent()?;
    }
}

#[cfg(test)]
#[path = "../../tests/git/os/path.rs"]
mod tests;

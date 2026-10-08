use git2::{Oid, Repository, Time};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadReflogEntry {
    pub selector: String,
    pub old_oid: Oid,
    pub new_oid: Oid,
    pub message: String,
    pub time: Time,
}

pub fn get_head_reflog_entries(repo: &Repository) -> Result<Vec<HeadReflogEntry>, git2::Error> {
    let reflog = repo.reflog("HEAD")?;
    let mut entries = Vec::new();

    for (idx, entry) in reflog.iter().enumerate() {
        let new_oid = entry.id_new();
        if new_oid.is_zero() || repo.find_commit(new_oid).is_err() {
            continue;
        }

        let message = entry.message().ok().flatten().map(str::to_string).or_else(|| entry.message_bytes().map(|bytes| String::from_utf8_lossy(bytes).to_string())).unwrap_or_else(|| "reflog".to_string());

        entries.push(HeadReflogEntry { selector: format!("HEAD@{{{idx}}}"), old_oid: entry.id_old(), new_oid, message, time: entry.committer().when() });
    }

    Ok(entries)
}

#[cfg(test)]
#[path = "../../tests/git/queries/reflogs.rs"]
mod tests;

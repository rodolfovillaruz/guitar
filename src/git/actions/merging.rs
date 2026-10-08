use crate::git::{
    actions::conflicts::{ensure_clean_workdir, mark_conflicts_resolved_from_workdir},
    queries::commits::get_current_branch,
};
use git2::{Error, MergeAnalysis, MergeOptions, Oid, Repository, RepositoryState, build::CheckoutBuilder};
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    Completed { oid: Oid },
    FastForward { oid: Oid },
    UpToDate,
    Conflict,
    Aborted,
}

pub fn is_merge_in_progress(repo: &Repository) -> bool {
    matches!(repo.state(), RepositoryState::Merge)
}

fn merge_options() -> MergeOptions {
    let mut opts = MergeOptions::new();
    opts.find_renames(true).standard_style(true);
    opts
}

fn checkout_options<'a>() -> CheckoutBuilder<'a> {
    let mut checkout = CheckoutBuilder::new();
    checkout.allow_conflicts(true).conflict_style_merge(true);
    checkout
}

fn force_checkout_options<'a>() -> CheckoutBuilder<'a> {
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    checkout
}

fn merge_head(repo: &Repository) -> Result<Oid, Error> {
    let path = repo.path().join("MERGE_HEAD");
    let message = fs::read_to_string(path).map_err(|error| Error::from_str(&format!("read MERGE_HEAD failed: {error}")))?;
    let oid = message.lines().next().ok_or_else(|| Error::from_str("MERGE_HEAD is empty"))?.trim();
    Oid::from_str(oid)
}

fn merge_message(repo: &Repository, target_oid: Oid) -> String {
    repo.message().ok().filter(|message| !message.trim().is_empty()).unwrap_or_else(|| format!("Merge commit '{target_oid}'"))
}

fn commit_merge(repo: &Repository, target_oid: Oid) -> Result<Oid, Error> {
    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let signature = repo.signature()?;
    let head_commit = repo.head()?.peel_to_commit()?;
    let target_commit = repo.find_commit(target_oid)?;
    let message = merge_message(repo, target_oid);
    let oid = repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[&head_commit, &target_commit])?;

    repo.cleanup_state()?;
    let mut checkout = force_checkout_options();
    repo.checkout_head(Some(&mut checkout))?;
    Ok(oid)
}

fn fast_forward(repo: &Repository, target_oid: Oid) -> Result<MergeOutcome, Error> {
    let mut head = repo.head()?;
    let head_name = head.name().map_err(|_| Error::from_str("HEAD reference name is not valid UTF-8"))?.to_string();
    let message = format!("Fast-forward: setting {head_name} to {target_oid}");

    head.set_target(target_oid, &message)?;
    repo.set_head(&head_name)?;
    let mut checkout = force_checkout_options();
    repo.checkout_head(Some(&mut checkout))?;
    Ok(MergeOutcome::FastForward { oid: target_oid })
}

fn normal_merge(repo: &Repository, target_oid: Oid) -> Result<MergeOutcome, Error> {
    let target = repo.find_annotated_commit(target_oid)?;
    let mut merge_opts = merge_options();
    let mut checkout_opts = checkout_options();
    repo.merge(&[&target], Some(&mut merge_opts), Some(&mut checkout_opts))?;

    if repo.index()?.has_conflicts() {
        return Ok(MergeOutcome::Conflict);
    }

    commit_merge(repo, target_oid).map(|oid| MergeOutcome::Completed { oid })
}

fn merge_is_possible(analysis: MergeAnalysis) -> bool {
    analysis.is_fast_forward() || analysis.is_normal() || analysis.is_up_to_date()
}

pub fn start_merge(repo: &Repository, target_oid: Oid) -> Result<MergeOutcome, Error> {
    if get_current_branch(repo).is_none() {
        return Err(Error::from_str("merging requires a checked-out local branch"));
    }
    if repo.state() != RepositoryState::Clean {
        return Err(Error::from_str("another git operation is already in progress"));
    }

    ensure_clean_workdir(repo, "merging")?;

    let target = repo.find_annotated_commit(target_oid)?;
    let (analysis, preference) = repo.merge_analysis(&[&target])?;

    if analysis.is_up_to_date() {
        return Ok(MergeOutcome::UpToDate);
    }
    if !merge_is_possible(analysis) {
        return Err(Error::from_str("merge is not possible"));
    }
    if preference.is_fastforward_only() && !analysis.is_fast_forward() {
        return Err(Error::from_str("merge.ff=only prevents a non-fast-forward merge"));
    }
    if analysis.is_fast_forward() && !preference.is_no_fast_forward() {
        return fast_forward(repo, target_oid);
    }

    normal_merge(repo, target_oid)
}

pub fn continue_merge(repo: &Repository) -> Result<MergeOutcome, Error> {
    if !is_merge_in_progress(repo) {
        return Err(Error::from_str("no merge in progress"));
    }

    mark_conflicts_resolved_from_workdir(repo)?;
    if repo.index()?.has_conflicts() {
        return Ok(MergeOutcome::Conflict);
    }

    let target_oid = merge_head(repo)?;
    commit_merge(repo, target_oid).map(|oid| MergeOutcome::Completed { oid })
}

pub fn abort_merge(repo: &Repository) -> Result<MergeOutcome, Error> {
    if !is_merge_in_progress(repo) {
        return Err(Error::from_str("no merge in progress"));
    }

    let mut checkout = force_checkout_options();
    repo.reset(&repo.head()?.peel_to_commit()?.into_object(), git2::ResetType::Hard, Some(&mut checkout))?;
    repo.cleanup_state()?;
    Ok(MergeOutcome::Aborted)
}

#[cfg(test)]
#[path = "../../tests/git/actions/merging.rs"]
mod tests;

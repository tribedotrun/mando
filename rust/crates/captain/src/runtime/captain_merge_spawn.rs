//! Spawn logic for captain merge sessions.
//!
//! A merge is attempted deterministically first: `gh pr merge --squash`
//! through the typed GitHub provider. Only when that is refused does captain
//! spawn a merge session, which is why the `captain_merge` prompt opens with
//! "the automatic squash-merge failed".

use anyhow::Result;
use rustc_hash::FxHashMap;

use global_github::{MergeBlockReason, MergeOutcome, ReviewDecision};
use settings::CaptainWorkflow;
use settings::Config;

use crate::Task;

use super::captain_merge::MergeResult;
use super::notify::Notifier;

/// What `spawn_merge` actually did.
pub(crate) enum MergeAttempt {
    /// The squash-merge went through; no session was started. The caller
    /// applies the result so the task reaches `Merged` the same way a
    /// session-driven merge does.
    Merged(MergeResult),
    /// GitHub refused the merge; a merge session is now running.
    SessionSpawned,
}

/// Attempt a merge for an item, spawning a session only if the deterministic
/// squash-merge is refused. Sets status to CaptainMerging when it spawns.
#[tracing::instrument(skip_all)]
pub(crate) async fn spawn_merge(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<MergeAttempt> {
    let cwd = item
        .worktree
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            config
                .captain
                .projects
                .values()
                .next()
                .map(|p| std::path::PathBuf::from(&p.path))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no CWD for captain merge: item has no worktree and no projects configured"
            )
        })?;

    let pr_num = item
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("cannot merge item without a PR"))?;

    let pr_number = pr_num.to_string();

    let repo = item
        .github_repo
        .clone()
        .or_else(|| settings::resolve_github_repo(Some(&item.project), config))
        .ok_or_else(|| anyhow::anyhow!("no github_repo for project {:?}", item.project))?;

    let pr_url = format!("https://github.com/{repo}/pull/{pr_number}");

    let subject = squash_subject(&item.title, &pr_number);

    if let Some(result) = try_squash_merge(item.id, &repo, &pr_number, &subject).await {
        return Ok(MergeAttempt::Merged(result));
    }

    // Render prompt before any side effects so failures propagate as Err
    // rather than dying silently inside tokio::spawn.
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("pr_url", pr_url.as_str());
    vars.insert("repo", repo.as_str());
    vars.insert("pr_number", pr_number.as_str());
    vars.insert("title", item.title.as_str());
    let prompt = settings::render_prompt("captain_merge", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("render captain_merge prompt: {e}"))?;

    super::agent_runtime::spawn_merge_session(
        item, &cwd, notifier, pool, &pr_url, &pr_number, &prompt, workflow,
    )
    .await?;
    Ok(MergeAttempt::SessionSpawned)
}

/// One squash-merge attempt, retried once with `--admin` when the only thing
/// in the way is branch protection *and* GitHub confirms no human review is
/// outstanding. An outstanding review falls through to the merge session,
/// whose prompt carries the never-bypass-reviews policy.
async fn try_squash_merge(
    item_id: i64,
    repo: &str,
    pr_number: &str,
    subject: &str,
) -> Option<MergeResult> {
    match attempt(item_id, repo, pr_number, subject, false).await {
        Some(MergeOutcome::Merged) => {}
        Some(MergeOutcome::Blocked { reason, detail }) if reason.admin_can_bypass() => {
            if admin_retry_would_bypass_review(item_id, repo, pr_number).await {
                return None;
            }
            tracing::info!(
                module = "captain",
                item_id,
                %detail,
                "squash-merge blocked by branch protection with no review outstanding; retrying with --admin"
            );
            match attempt(item_id, repo, pr_number, subject, true).await {
                Some(MergeOutcome::Merged) => {}
                _ => return None,
            }
        }
        Some(MergeOutcome::Blocked { reason, detail }) => {
            tracing::info!(
                module = "captain",
                item_id,
                reason = merge_block_label(reason),
                %detail,
                "squash-merge refused; falling back to a merge session"
            );
            return None;
        }
        None => return None,
    }

    Some(MergeResult {
        action: "merged".into(),
        feedback: format!("squash-merged {repo}#{pr_number}"),
    })
}

/// The squash-merge commit subject, matching the repo's history convention.
///
/// GitHub derives `<title> (#<pr>)` on its own only for a multi-commit PR; a
/// single-commit PR inherits the lone commit's subject instead and loses the
/// PR reference, so captain always states the subject.
fn squash_subject(title: &str, pr_number: &str) -> String {
    format!("{title} (#{pr_number})")
}

/// Whether an `--admin` retry would clear a human review along with branch
/// protection.
///
/// gh's client-side precheck collapses every protection rule into one
/// "base branch policy prohibits the merge" message, so
/// [`MergeBlockReason::BranchProtection`] on its own is not evidence that a
/// required approval is satisfied. Ask GitHub directly.
async fn admin_retry_would_bypass_review(item_id: i64, repo: &str, pr_number: &str) -> bool {
    let decision = match global_github::pr_review_decision(repo, pr_number).await {
        Ok(decision) => Some(decision),
        Err(e) => {
            tracing::warn!(
                module = "captain",
                item_id,
                error = %e,
                "could not read reviewDecision; skipping the --admin retry"
            );
            None
        }
    };
    let blocks = blocks_admin_retry(decision);
    if blocks {
        if let Some(decision) = decision {
            tracing::info!(
                module = "captain",
                item_id,
                review_decision = decision.as_str(),
                "squash-merge blocked with a human review outstanding; \
                 skipping the --admin retry and falling back to a merge session"
            );
        }
    }
    blocks
}

/// Fail closed: `None` means the decision could not be read, which is not
/// evidence that overriding is safe. Only a decision GitHub returned, and
/// that names no outstanding review, clears the retry.
fn blocks_admin_retry(decision: Option<ReviewDecision>) -> bool {
    match decision {
        Some(decision) => decision.blocks_admin_merge(),
        None => true,
    }
}

async fn attempt(
    item_id: i64,
    repo: &str,
    pr_number: &str,
    subject: &str,
    admin: bool,
) -> Option<MergeOutcome> {
    match global_github::merge_pr_squash(repo, pr_number, subject, admin).await {
        Ok(outcome) => Some(outcome),
        Err(e) => {
            tracing::warn!(
                module = "captain",
                item_id,
                error = %e,
                "gh pr merge could not run; falling back to a merge session"
            );
            None
        }
    }
}

fn merge_block_label(reason: MergeBlockReason) -> &'static str {
    match reason {
        MergeBlockReason::BranchProtection => "branch_protection",
        MergeBlockReason::ReviewRequired => "review_required",
        MergeBlockReason::NotMergeable => "not_mergeable",
        MergeBlockReason::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_branch_protection_is_admin_bypassable() {
        assert!(MergeBlockReason::BranchProtection.admin_can_bypass());
        for reason in [
            MergeBlockReason::ReviewRequired,
            MergeBlockReason::NotMergeable,
            MergeBlockReason::Other,
        ] {
            assert!(
                !reason.admin_can_bypass(),
                "{} must never be bypassed with --admin",
                merge_block_label(reason)
            );
        }
    }

    /// gh's precheck reports a missing approval through the same umbrella
    /// message as a failing status check, so `admin_can_bypass` alone would
    /// let `--admin` clear a required human review. The reviewDecision gate
    /// is what makes that impossible.
    #[test]
    fn an_outstanding_review_blocks_the_admin_retry() {
        assert!(blocks_admin_retry(Some(ReviewDecision::ReviewRequired)));
        assert!(blocks_admin_retry(Some(ReviewDecision::ChangesRequested)));
    }

    #[test]
    fn a_satisfied_review_allows_the_admin_retry() {
        assert!(!blocks_admin_retry(Some(ReviewDecision::Approved)));
        assert!(!blocks_admin_retry(Some(ReviewDecision::NotRequired)));
    }

    #[test]
    fn an_unreadable_review_decision_blocks_the_admin_retry() {
        assert!(
            blocks_admin_retry(None),
            "a decision captain could not read is not proof that overriding is safe"
        );
    }

    #[test]
    fn the_squash_subject_carries_the_pr_reference() {
        assert_eq!(
            squash_subject("Fix rewards code list scrolling", "552"),
            "Fix rewards code list scrolling (#552)"
        );
    }

    #[test]
    fn every_block_reason_has_a_log_label() {
        for reason in [
            MergeBlockReason::BranchProtection,
            MergeBlockReason::ReviewRequired,
            MergeBlockReason::NotMergeable,
            MergeBlockReason::Other,
        ] {
            assert!(!merge_block_label(reason).is_empty());
        }
    }
}

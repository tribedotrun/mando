use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct PrStatus {
    pub author: String,
    pub ci_status: Option<String>,
    pub comments: i64,
    pub unresolved_threads: i64,
    pub unreplied_threads: i64,
    pub unaddressed_issue_comments: i64,
    pub body: String,
    pub head_sha: String,
    pub is_draft: bool,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeableStatus {
    Merged,
    Closed,
    Mergeable,
    Conflicted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
    Unknown(String),
}

/// Result of a deterministic squash-merge attempt.
///
/// A refused merge is an expected outcome, not an error: `Err` stays reserved
/// for transport failures (`gh` missing or unable to run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// `gh pr merge --squash` exited zero; the PR is merged.
    Merged,
    /// `gh` refused the merge. `detail` carries gh's own trimmed output so the
    /// caller can log or relay the verbatim reason.
    Blocked {
        reason: MergeBlockReason,
        detail: String,
    },
}

/// Why a squash-merge was refused, classified from gh's own output.
///
/// Deliberately coarse: anything this crate does not positively recognize is
/// [`MergeBlockReason::Other`], never a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeBlockReason {
    /// Branch protection or required status checks refused the merge — the
    /// one class an administrator override is capable of clearing.
    BranchProtection,
    /// A required human approving review is missing, or changes were
    /// requested. Never clear this with `--admin`.
    ReviewRequired,
    /// The pull request cannot be merged as it stands (conflicts, dirty merge
    /// state, base branch moved). Needs a rebase or new commits, not a flag.
    NotMergeable,
    /// gh refused for a reason this crate does not recognize. Treat as
    /// human-decidable; inspect `detail`.
    Other,
}

impl MergeBlockReason {
    /// Whether `--admin` is *capable* of clearing this class of block.
    ///
    /// A capability hint only — retry policy belongs to the caller. This is
    /// always `false` for [`MergeBlockReason::ReviewRequired`]; a missing
    /// human approval must never be bypassed. A `true` answer still leaves the
    /// caller responsible for confirming no human review is outstanding,
    /// because gh's generic branch-policy message names no specific rule:
    /// call [`crate::pr_review_decision`] and check
    /// [`ReviewDecision::blocks_admin_merge`] first.
    #[must_use]
    pub fn admin_can_bypass(self) -> bool {
        matches!(self, Self::BranchProtection)
    }
}

/// GitHub's aggregate review state for a pull request (`reviewDecision`).
///
/// Unlike gh's refusal text, this is a machine-readable field: it answers
/// "is a human approval still outstanding?" directly, which the umbrella
/// branch-policy message cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
    /// The repository requires no review on this PR, or GitHub reported an
    /// empty decision.
    NotRequired,
}

impl ReviewDecision {
    /// Whether a human review still stands between this PR and a merge.
    ///
    /// The one question an `--admin` retry must answer before it runs:
    /// administrator override clears branch protection, and it would clear a
    /// required approval along with it.
    #[must_use]
    pub fn blocks_admin_merge(self) -> bool {
        matches!(self, Self::ReviewRequired | Self::ChangesRequested)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::ChangesRequested => "CHANGES_REQUESTED",
            Self::ReviewRequired => "REVIEW_REQUIRED",
            Self::NotRequired => "none",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PrComment {
    #[serde(alias = "author", deserialize_with = "deserialize_author_lenient")]
    pub user: String,
    pub body: String,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ThreadComment {
    pub author: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ReviewThread {
    pub is_resolved: bool,
    pub comments: Vec<ThreadComment>,
}

fn deserialize_author_lenient<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match val {
        Some(serde_json::Value::Object(map)) => map
            .get("login")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(serde_json::Value::String(s)) => s,
        _ => String::new(),
    })
}

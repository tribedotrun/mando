#!/usr/bin/env python3
"""PR ship watcher for checks, comments, reviewers, review windows, and Actions."""

from __future__ import annotations

import argparse
import asyncio
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Callable

from gh_async import (
    dedupe_check_runs,
    detect_repo,
    fetch_all,
    get_check_runs,
    get_pr_head_info,
    get_workflow_runs,
    run_gh,
)

REVIEWERS = {
    "codex": {"login": "chatgpt-codex-connector[bot]"},
    "cursor": {"login": "cursor[bot]"},
    "devin": {"login": "devin-ai-integration[bot]"},
}
TRIGGER_PATTERNS = {
    "codex": ["@codex review", "@codex r"],
    "cursor": ["cursor review", "bugbot run"],
}
WATCH_POLL_SECONDS = 15
WATCH_HEARTBEAT_SECONDS = 60
WATCH_MAX_WAIT_SECONDS = 300
PASS_CONCLUSIONS = {"success", "skipped", "neutral"}
UNAVAILABLE_PATTERNS = [
    "usage limit reached",
    "reached your usage limit",
    "add credits to continue",
    "quota exceeded",
    "quota exhausted",
    "review quota reached",
    "plan limit reached",
    "bugbot is disabled for this repository",
]
ERROR_PATTERNS = [
    "rate limit",
    "service unavailable",
    "something went wrong",
    "internal error",
    "temporarily unavailable",
]
CODEX_REVIEWED_COMMIT_RE = re.compile(
    r"(?:\*\*)?Reviewed commit:(?:\*\*)?\s*`?([0-9a-f]{7,40})`?",
    re.IGNORECASE,
)
CODEX_UNAVAILABLE_PATTERNS = [
    "you have reached your codex usage limits for code reviews",
]
CODEX_ERROR_PATTERNS = [
    "codex review failed",
    "codex was unable to complete the review",
    "codex could not complete the review",
    "codex couldn't complete the review",
    "codex review service is unavailable",
    "codex review is temporarily unavailable",
    "codex review rate limit exceeded",
    "code review service is unavailable",
    "code reviews are temporarily unavailable",
]


@dataclass(frozen=True)
class StatusResult:
    exit_code: int
    has_wait: bool
    has_actionable: bool

    @property
    def wait_only(self) -> bool:
        return self.exit_code != 0 and self.has_wait and not self.has_actionable


@dataclass(frozen=True)
class StatusSnapshot:
    head_sha: str
    pushed_at: str | None
    values: tuple[tuple[str, str], ...]
    details: tuple[str, ...]
    has_wait: bool
    has_actionable: bool
    window_deadline: datetime | None

    @property
    def result(self) -> StatusResult:
        clear = not self.has_wait and not self.has_actionable
        return StatusResult(0 if clear else 1, self.has_wait, self.has_actionable)

    def value_map(self) -> dict[str, str]:
        return dict(self.values)


def _item_timestamp(item: dict) -> str:
    return (
        item.get("submitted_at")
        or item.get("created_at")
        or item.get("updated_at")
        or ""
    )


def _timestamp_sort_key(raw: str) -> datetime:
    if not raw:
        return datetime.min.replace(tzinfo=timezone.utc)
    return _parse_timestamp(raw, "GitHub response timestamp")


def _parse_timestamp(raw: str, label: str) -> datetime:
    normalized = raw.strip().replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise RuntimeError(f"{label}: expected an ISO-8601 timestamp, got {raw!r}") from exc
    if parsed.tzinfo is None:
        raise RuntimeError(f"{label}: timestamp must include a timezone")
    return parsed.astimezone(timezone.utc)


def _is_unavailable_response(body: str) -> bool:
    lowered = body.lower()
    return any(pattern in lowered for pattern in UNAVAILABLE_PATTERNS)


def _is_error_response(body: str) -> bool:
    lowered = body.lower()
    return any(pattern in lowered for pattern in ERROR_PATTERNS)


async def get_pr_author(pr: int) -> str:
    raw = await run_gh("pr", "view", str(pr), "--json", "author", "-q", ".author.login")
    return raw.strip()


def find_unaddressed_comments(data: dict, pr_author: str) -> list[dict]:
    """Find unresolved, non-outdated review threads without an author reply."""
    review_comments = data.get("review_comments", [])
    thread_meta = data.get("thread_meta", {})
    threads: dict[int, list[dict]] = {}
    for comment in review_comments:
        root_id = comment.get("in_reply_to_id") or comment["id"]
        threads.setdefault(root_id, []).append(comment)

    unaddressed: list[dict] = []
    for root_id, thread in threads.items():
        meta = thread_meta.get(root_id, {})
        if meta.get("is_resolved") or meta.get("is_outdated"):
            continue
        root = next((comment for comment in thread if comment["id"] == root_id), thread[0])
        if root.get("user", {}).get("login") == pr_author:
            continue
        author_replied = any(
            comment.get("user", {}).get("login") == pr_author and comment["id"] != root_id
            for comment in thread
        )
        if not author_replied:
            unaddressed.append(root)
    return unaddressed


def latest_reviewer_triggers(data: dict) -> dict[str, str]:
    """Return each explicitly triggered reviewer and latest request timestamp."""
    latest: dict[str, str] = {}
    for item in data.get("comments", []):
        body = (item.get("body") or "").lower()
        timestamp = _item_timestamp(item)
        for name, patterns in TRIGGER_PATTERNS.items():
            if (
                any(pattern in body for pattern in patterns)
                and _timestamp_sort_key(timestamp)
                >= _timestamp_sort_key(latest.get(name, ""))
            ):
                latest[name] = timestamp
    if "devin" not in latest:
        timestamps = [
            _item_timestamp(item)
            for item in data.get("reviews", [])
            if item.get("user", {}).get("login") == REVIEWERS["devin"]["login"]
        ]
        if timestamps:
            latest["devin"] = ""
    return latest


def detect_triggered_reviewers(data: dict) -> list[str]:
    return list(latest_reviewer_triggers(data))


def _item_matches_head(item: dict, head_sha: str) -> bool:
    """Require every commit anchor an API item supplies to match the PR head."""
    if not head_sha:
        return True
    anchors = [item.get("commit_id"), item.get("original_commit_id")]
    supplied = [str(anchor).lower() for anchor in anchors if anchor]
    return not supplied or all(anchor == head_sha.lower() for anchor in supplied)


def _issue_comment_terminal_state(
    name: str,
    body: str,
    head_sha: str,
    has_current_head_anchor: bool = False,
    allow_unanchored_failures: bool = True,
) -> str | None:
    """Recognize reviewer-specific terminal issue comments without commit fields."""
    lowered = body.lower()
    if name == "codex":
        marker = CODEX_REVIEWED_COMMIT_RE.search(body)
        if marker is not None:
            reviewed = marker.group(1).lower()
            if head_sha and head_sha.lower().startswith(reviewed):
                return "real"
            return None
        normalized = lowered.strip()
        if not allow_unanchored_failures:
            return None
        if any(normalized.startswith(pattern) for pattern in CODEX_UNAVAILABLE_PATTERNS):
            return "unavailable"
        if any(normalized.startswith(pattern) for pattern in CODEX_ERROR_PATTERNS):
            return "error"
        return None
    elif name == "cursor":
        if "bugbot" not in lowered:
            return None
    else:
        return None
    if _is_unavailable_response(body):
        return "unavailable"
    if _is_error_response(body):
        return "error"
    if name == "cursor" and not has_current_head_anchor:
        return None
    return "real"


def check_reviewer_status(
    name: str,
    data: dict,
    not_before: str = "",
    head_sha: str = "",
    allow_issue_comments: bool = True,
    allow_unanchored_failures: bool = True,
) -> str:
    """Return ``real``, ``unavailable``, ``error``, or ``none`` for a reviewer."""
    login = REVIEWERS[name]["login"]
    latest_item = None
    latest_timestamp = ""
    for source, items in (
        ("review", data.get("reviews", [])),
        ("issue", data.get("comments", [])),
    ):
        if source == "issue" and not allow_issue_comments:
            continue
        for item in items:
            if item.get("user", {}).get("login") != login:
                continue
            if not _item_matches_head(item, head_sha):
                continue
            if source == "review" and not item.get("submitted_at"):
                continue
            terminal_state = (
                "review"
                if source == "review"
                else _issue_comment_terminal_state(
                    name,
                    item.get("body") or "",
                    head_sha,
                    has_current_head_anchor=bool(
                        item.get("commit_id") or item.get("original_commit_id")
                    ),
                    allow_unanchored_failures=allow_unanchored_failures,
                )
            )
            if terminal_state is None:
                continue
            timestamp = _item_timestamp(item)
            if (
                _timestamp_sort_key(timestamp) < _timestamp_sort_key(not_before)
                or _timestamp_sort_key(timestamp) < _timestamp_sort_key(latest_timestamp)
            ):
                continue
            latest_timestamp = timestamp
            latest_item = (item, terminal_state)
    if latest_item is None:
        return "none"
    item, terminal_state = latest_item
    if terminal_state != "review":
        return terminal_state
    body = item.get("body") or ""
    if _is_unavailable_response(body):
        return "unavailable"
    if _is_error_response(body):
        return "error"
    return "real"


def _required_check(name: str, checks_policy: str) -> bool:
    return checks_policy == "required-prefix" and name.startswith("checks")


def _workflow_value(
    run: dict,
    expected_head_sha: str | None = None,
) -> tuple[str, str, bool, bool]:
    run_id = str(run.get("id") or "?")
    name = run.get("name") or run.get("display_title") or "workflow"
    status = run.get("status") or "unknown"
    conclusion = run.get("conclusion")
    run_head_sha = run.get("head_sha")
    if expected_head_sha and run_head_sha != expected_head_sha:
        actual = str(run_head_sha or "missing")[:8]
        return (
            f"workflow:{run_id}",
            f"FAIL {name}: head {actual} != {expected_head_sha[:8]}",
            False,
            True,
        )
    if status != "completed":
        return f"workflow:{run_id}", f"WAIT {name}", True, False
    if conclusion in PASS_CONCLUSIONS:
        return f"workflow:{run_id}", f"PASS {name}", False, False
    return f"workflow:{run_id}", f"FAIL {name}: {conclusion or 'unknown'}", False, True


def _window_deadline(
    explicit_start: str | None,
    review_window_seconds: int,
    observed_at: datetime | None = None,
) -> datetime | None:
    if review_window_seconds == 0:
        return None
    if explicit_start:
        start = _parse_timestamp(explicit_start, "review window start")
    elif observed_at:
        start = observed_at.astimezone(timezone.utc)
    else:
        raise RuntimeError("review window requires --window-start or a watch observation")
    return start + timedelta(seconds=review_window_seconds)


async def collect_status(
    owner: str,
    repo: str,
    pr: int,
    wanted: list[str] | None,
    *,
    run_ids: list[int] | None = None,
    review_window_seconds: int = 0,
    window_start: str | None = None,
    checks_policy: str = "required-prefix",
    expected_head_sha: str | None = None,
    observed_at: datetime | None = None,
    now: Callable[[], datetime] = lambda: datetime.now(timezone.utc),
) -> StatusSnapshot:
    pr_author_task = get_pr_author(pr)
    head_task = get_pr_head_info(owner, repo, pr)
    data_task = fetch_all(owner, repo, pr)
    pr_author, head, data = await asyncio.gather(pr_author_task, head_task, data_task)
    if not pr_author:
        raise RuntimeError("Could not determine PR author")
    head_sha = str(head["sha"])
    pushed_at = head.get("pushed_at")
    checks, workflow_runs = await asyncio.gather(
        get_check_runs(owner, repo, head_sha),
        get_workflow_runs(owner, repo, run_ids or []),
    )

    values: dict[str, str] = {"head": head_sha[:8]}
    details: list[str] = []
    has_wait = False
    has_actionable = False

    if expected_head_sha and head_sha != expected_head_sha:
        values["head"] = f"CHANGED {expected_head_sha[:8]} -> {head_sha[:8]}"
        details.append("The PR head changed while watching; rerun gates and reviewer triggers.")
        has_actionable = True

    for check in dedupe_check_runs(checks):
        name = check.get("name") or "?"
        status = check.get("status") or "?"
        conclusion = check.get("conclusion") or "?"
        required = _required_check(name, checks_policy)
        key = f"check:{name}"
        if status != "completed":
            values[key] = f"WAIT {status}" if required else f"INFO {status}"
            has_wait = has_wait or required
        elif conclusion in PASS_CONCLUSIONS:
            values[key] = f"PASS {conclusion}"
        elif required:
            values[key] = f"FAIL {conclusion}"
            has_actionable = True
        else:
            values[key] = f"INFO {conclusion}"

    for comment in find_unaddressed_comments(data, pr_author):
        comment_id = comment.get("id", "?")
        author = comment.get("user", {}).get("login", "?")
        path = comment.get("path", "?")
        line = comment.get("line") or comment.get("original_line") or "?"
        body = (comment.get("body") or "").strip().split("\n")[0][:200] or "(empty)"
        values[f"comment:{comment_id}"] = f"ACTION @{author} {path}:{line}"
        details.append(f"#{comment_id} @{author} {path}:{line} — {body}")
        has_actionable = True

    trigger_times = latest_reviewer_triggers(data)
    selected = list(trigger_times) if wanted is None else wanted
    for name in selected:
        trigger_time = trigger_times.get(name, "")
        head_baseline = window_start or ""
        if not head_baseline:
            status = check_reviewer_status(
                name,
                data,
                trigger_time,
                head_sha,
                allow_issue_comments=True,
                allow_unanchored_failures=bool(trigger_time),
            )
            if status == "none":
                values[f"reviewer:{name}"] = "WAIT response"
                has_wait = True
                continue
            if status == "unavailable":
                values[f"reviewer:{name}"] = "UNAVAILABLE reviewer"
            elif status == "error":
                values[f"reviewer:{name}"] = "ACTION transient error"
                has_actionable = True
            else:
                values[f"reviewer:{name}"] = "DONE"
            continue
        trigger_is_stale = bool(
            trigger_time
            and _timestamp_sort_key(trigger_time) < _timestamp_sort_key(head_baseline)
        )
        cutoff = (
            head_baseline
            if trigger_is_stale
            else max((trigger_time, head_baseline), key=_timestamp_sort_key)
        )
        status = check_reviewer_status(name, data, cutoff, head_sha)
        if trigger_is_stale and status == "none":
            values[f"reviewer:{name}"] = "ACTION trigger predates head"
            has_actionable = True
            continue
        if status == "none":
            values[f"reviewer:{name}"] = "WAIT response"
            has_wait = True
        elif status == "unavailable":
            values[f"reviewer:{name}"] = "UNAVAILABLE reviewer"
        elif status == "error":
            values[f"reviewer:{name}"] = "ACTION transient error"
            has_actionable = True
        else:
            values[f"reviewer:{name}"] = "DONE"

    for run in workflow_runs:
        key, value, waiting, actionable = _workflow_value(run, head_sha)
        values[key] = value
        has_wait = has_wait or waiting
        has_actionable = has_actionable or actionable

    current = now().astimezone(timezone.utc)
    deadline = _window_deadline(
        window_start,
        review_window_seconds,
        observed_at=observed_at or current,
    )
    if deadline:
        if current < deadline:
            values["review-window"] = f"WAIT until {deadline.isoformat()}"
            has_wait = True
        else:
            values["review-window"] = f"READY since {deadline.isoformat()}"

    return StatusSnapshot(
        head_sha=head_sha,
        pushed_at=pushed_at,
        values=tuple(sorted(values.items())),
        details=tuple(details),
        has_wait=has_wait,
        has_actionable=has_actionable,
        window_deadline=deadline,
    )


def render_full(owner: str, repo: str, pr: int, snapshot: StatusSnapshot) -> None:
    print(f"PR #{pr} ({owner}/{repo}) head: {snapshot.head_sha[:8]}")
    grouped = {
        "CI": "check:",
        "COMMENTS": "comment:",
        "REVIEWERS": "reviewer:",
        "WORKFLOW RUNS": "workflow:",
    }
    values = snapshot.value_map()
    for title, prefix in grouped.items():
        entries = [(key, value) for key, value in snapshot.values if key.startswith(prefix)]
        print(f"\n{title}:")
        if not entries:
            print("  (none)")
        for key, value in entries:
            print(f"  {key.removeprefix(prefix)}: {value}")
    if "review-window" in values:
        print(f"\nREVIEW WINDOW:\n  {values['review-window']}")
    for detail in snapshot.details:
        print(f"  {detail}")
    print("\nALL CLEAR" if snapshot.result.exit_code == 0 else "")


def render_changes(previous: StatusSnapshot, current: StatusSnapshot) -> None:
    before = previous.value_map()
    after = current.value_map()
    changes: list[str] = []
    changed_keys: set[str] = set()
    for key in sorted(set(before) | set(after)):
        old = before.get(key)
        new = after.get(key)
        if old == new:
            continue
        changed_keys.add(key)
        if old is None:
            changes.append(f"  {key}: {new}")
        elif new is None:
            changes.append(f"  {key}: {old} -> cleared")
        else:
            changes.append(f"  {key}: {old} -> {new}")
    if changes:
        print("STATE CHANGES:")
        print("\n".join(changes))
        for detail in current.details:
            comment_id = detail.split(" ", 1)[0].removeprefix("#")
            if f"comment:{comment_id}" in changed_keys:
                print(f"  {detail}")
        print()


def render_heartbeat(snapshot: StatusSnapshot) -> None:
    waiting = [key for key, value in snapshot.values if value.startswith("WAIT")]
    window = ""
    if snapshot.window_deadline:
        remaining = max(
            0,
            int((snapshot.window_deadline - datetime.now(timezone.utc)).total_seconds()),
        )
        window = f"; review-window={remaining}s"
    print(
        f"HEARTBEAT: head {snapshot.head_sha[:8]}; "
        f"waiting={len(waiting)}; actionable={snapshot.has_actionable}{window}"
    )


async def check_status(
    owner: str,
    repo: str,
    pr: int,
    wanted: list[str] | None,
    **options,
) -> StatusResult:
    snapshot = await collect_status(owner, repo, pr, wanted, **options)
    render_full(owner, repo, pr, snapshot)
    return snapshot.result


async def watch_status(
    owner: str,
    repo: str,
    pr: int,
    wanted: list[str] | None,
    *,
    run_ids: list[int] | None = None,
    review_window_seconds: int = 0,
    window_start: str | None = None,
    checks_policy: str = "required-prefix",
    poll_seconds: float = WATCH_POLL_SECONDS,
    heartbeat_seconds: float = WATCH_HEARTBEAT_SECONDS,
    max_wait_seconds: float | None = None,
) -> int:
    loop = asyncio.get_running_loop()
    started_at = loop.time()
    observed_at = datetime.now(timezone.utc)
    last_heartbeat = started_at
    timeout = max_wait_seconds
    if timeout is None:
        timeout = max(WATCH_MAX_WAIT_SECONDS, review_window_seconds + poll_seconds)

    initial = await collect_status(
        owner,
        repo,
        pr,
        wanted,
        run_ids=run_ids,
        review_window_seconds=review_window_seconds,
        window_start=window_start,
        checks_policy=checks_policy,
        observed_at=observed_at,
    )
    render_full(owner, repo, pr, initial)
    if initial.result.exit_code == 0 or not initial.result.wait_only:
        return initial.result.exit_code

    expected_head = initial.head_sha
    previous = initial
    while True:
        elapsed = loop.time() - started_at
        remaining = timeout - elapsed
        if remaining <= 0:
            print(f"Timed out after {elapsed:.0f}s while PR had only wait-state blockers.")
            return 1
        await asyncio.sleep(min(poll_seconds, remaining))
        current = await collect_status(
            owner,
            repo,
            pr,
            wanted,
            run_ids=run_ids,
            review_window_seconds=review_window_seconds,
            window_start=window_start,
            checks_policy=checks_policy,
            expected_head_sha=expected_head,
            observed_at=observed_at,
        )
        render_changes(previous, current)
        current_time = loop.time()
        if current_time - last_heartbeat >= heartbeat_seconds:
            render_heartbeat(current)
            last_heartbeat = current_time
        if current.result.exit_code == 0:
            print("ALL CLEAR")
            return 0
        if not current.result.wait_only:
            return 1
        previous = current


def parse_reviewers(raw: str) -> list[str] | None:
    if raw == "auto":
        return None
    wanted = [reviewer.strip().lower() for reviewer in raw.split(",") if reviewer.strip()]
    for reviewer in wanted:
        if reviewer not in REVIEWERS:
            print(
                f"error: unknown reviewer '{reviewer}'. Known: {', '.join(REVIEWERS)}",
                file=sys.stderr,
            )
            raise SystemExit(2)
    return wanted


def non_negative_int(raw: str) -> int:
    value = int(raw)
    if value < 0:
        raise argparse.ArgumentTypeError("must be non-negative")
    return value


def positive_float(raw: str) -> float:
    value = float(raw)
    if value <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(
        description="PR ship watcher — checks, comments, reviewers, review window, and Actions",
    )
    parser.add_argument("pr", type=int, help="PR number")
    parser.add_argument(
        "--reviewers",
        default="auto",
        help="Comma-separated reviewer names, or 'auto' to detect",
    )
    parser.add_argument("--watch", action="store_true", help="Poll wait-only states internally")
    parser.add_argument("--run-id", type=int, action="append", default=[], help="Actions run ID (repeatable)")
    parser.add_argument("--review-window-seconds", type=non_negative_int, default=0)
    parser.add_argument(
        "--window-start",
        help="Exact final-push ISO-8601 time; otherwise watch conservatively from first observation",
    )
    parser.add_argument(
        "--checks-policy",
        choices=("required-prefix", "informational"),
        default="required-prefix",
        help="Whether checks* runs block or all PR checks are informational",
    )
    parser.add_argument("--poll-seconds", type=positive_float, default=WATCH_POLL_SECONDS)
    parser.add_argument("--heartbeat-seconds", type=positive_float, default=WATCH_HEARTBEAT_SECONDS)
    parser.add_argument("--max-wait-seconds", type=positive_float)
    args = parser.parse_args()
    wanted = parse_reviewers(args.reviewers)
    options = {
        "run_ids": args.run_id,
        "review_window_seconds": args.review_window_seconds,
        "window_start": args.window_start,
        "checks_policy": args.checks_policy,
    }

    async def run() -> int:
        owner, repo = await detect_repo()
        if args.watch:
            return await watch_status(
                owner,
                repo,
                args.pr,
                wanted,
                poll_seconds=args.poll_seconds,
                heartbeat_seconds=args.heartbeat_seconds,
                max_wait_seconds=args.max_wait_seconds,
                **options,
            )
        return (await check_status(owner, repo, args.pr, wanted, **options)).exit_code

    try:
        return asyncio.run(run())
    except RuntimeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""PR status check — one-shot or wait-aware report of CI, comments, and reviewers.

Default mode returns the current PR state so the caller can decide what to do.
Watch mode re-checks wait-only states internally and exits as soon as the PR is
clear or needs an agent action.

Exit codes:
  0 — All clear (CI green or non-blocking, no unaddressed comments, all reviewers responded)
  1 — Has issues, timed out, or needs agent action (details in stdout)
"""

from __future__ import annotations

import argparse
import asyncio
import sys
from dataclasses import dataclass

from gh_async import (
    dedupe_check_runs,
    detect_repo,
    fetch_all,
    get_check_runs,
    get_pr_head_sha,
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
WATCH_MAX_WAIT_SECONDS = 300

ERROR_PATTERNS = [
    "usage limit",
    "reached your",
    "add credits",
    "rate limit",
    "service unavailable",
    "something went wrong",
    "internal error",
    "temporarily unavailable",
]


@dataclass(frozen=True)
class StatusResult:
    exit_code: int
    has_wait: bool
    has_actionable: bool

    @property
    def wait_only(self) -> bool:
        return self.exit_code != 0 and self.has_wait and not self.has_actionable


def _is_error_response(body: str) -> bool:
    return any(pat in body.lower() for pat in ERROR_PATTERNS)


async def get_pr_author(pr: int) -> str:
    raw = await run_gh("pr", "view", str(pr), "--json", "author", "-q", ".author.login")
    return raw.strip()


def find_unaddressed_comments(data: dict, pr_author: str) -> list[dict]:
    """Find review-line threads that still need PR-author action.

    A thread is addressed when any of the following hold:
      1. GitHub marks it resolved (reviewer hit "Resolve conversation").
      2. GitHub marks it outdated (anchor line no longer in the diff).
      3. The PR author opened the thread.
      4. The PR author has already replied to a reviewer's root comment.
    """
    review_comments = data.get("review_comments", [])
    thread_meta = data.get("thread_meta", {})
    threads: dict[int, list[dict]] = {}
    for c in review_comments:
        root_id = c.get("in_reply_to_id") or c["id"]
        threads.setdefault(root_id, []).append(c)

    unaddressed: list[dict] = []
    for root_id, thread in threads.items():
        meta = thread_meta.get(root_id, {})
        if meta.get("is_resolved") or meta.get("is_outdated"):
            continue
        root = next((c for c in thread if c["id"] == root_id), thread[0])
        if root.get("user", {}).get("login") == pr_author:
            continue
        has_reply = any(
            c.get("user", {}).get("login") == pr_author and c["id"] != root_id for c in thread
        )
        if not has_reply:
            unaddressed.append(root)
    return unaddressed


def detect_triggered_reviewers(data: dict) -> list[str]:
    triggered: list[str] = []
    all_text = ""
    for item in data["comments"]:
        all_text += " " + (item.get("body") or "").lower()
    for name, patterns in TRIGGER_PATTERNS.items():
        for pat in patterns:
            if pat.lower() in all_text:
                triggered.append(name)
                break
    if "devin" not in triggered:
        for item in data["review_comments"]:
            if item.get("user", {}).get("login") == REVIEWERS["devin"]["login"]:
                triggered.append("devin")
                break
    return triggered


def check_reviewer_status(name: str, data: dict) -> str:
    """Returns 'real', 'error', or 'none' based on the reviewer's latest response."""
    login = REVIEWERS[name]["login"]
    latest_item = None
    latest_ts = ""
    for item in data["reviews"] + data["review_comments"] + data["comments"]:
        if item.get("user", {}).get("login") != login:
            continue
        ts = item.get("submitted_at") or item.get("updated_at") or item.get("created_at") or ""
        if ts >= latest_ts:
            latest_ts = ts
            latest_item = item
    if latest_item is None:
        return "none"
    body = latest_item.get("body", "")
    if body and _is_error_response(body):
        return "error"
    return "real"


async def check_status(
    owner: str,
    repo: str,
    pr: int,
    wanted: list[str] | None,
) -> StatusResult:
    pr_author = await get_pr_author(pr)
    if not pr_author:
        raise RuntimeError("Could not determine PR author")
    head_sha = await get_pr_head_sha(pr)

    data = await fetch_all(owner, repo, pr)
    runs = await get_check_runs(owner, repo, head_sha)

    if wanted is None:
        wanted = detect_triggered_reviewers(data)

    has_wait = False
    has_actionable = False

    # --- CI ---
    # Only checks whose name starts with "checks" are required CI.
    # Review bots (Greptile, Devin, CodeRabbit, Codex) are informational.
    print(f"PR #{pr} ({owner}/{repo}) head: {head_sha[:8]}\n")
    print("CI:")
    if runs:
        deduped = dedupe_check_runs(runs)
        for r in deduped:
            name = r.get("name", "?")
            status = r.get("status", "?")
            conclusion = r.get("conclusion", "?")
            required = name.startswith("checks")
            if status != "completed":
                if required:
                    print(f"  [WAIT] {name}")
                    has_wait = True
                else:
                    print(f"  [INFO] {name} (pending, non-blocking)")
            elif conclusion in ("success", "skipped", "neutral"):
                print(f"  [PASS] {name}")
            else:
                if required:
                    print(f"  [FAIL] {name}: {conclusion}")
                    has_actionable = True
                else:
                    print(f"  [INFO] {name}: {conclusion} (non-blocking)")
    else:
        print("  (no checks detected)")

    # --- Unaddressed comments ---
    unaddressed = find_unaddressed_comments(data, pr_author)
    if unaddressed:
        has_actionable = True
        print(f"\nUNADDRESSED COMMENTS ({len(unaddressed)}):")
        for c in unaddressed:
            cid = c.get("id", "?")
            author = c.get("user", {}).get("login", "?")
            path = c.get("path", "?")
            line = c.get("line") or c.get("original_line") or "?"
            body = (c.get("body") or "")[:200]
            first_line = body.strip().split("\n")[0] if body.strip() else "(empty)"
            print(f"  #{cid} @{author} {path}:{line}")
            print(f"    {first_line}")
    else:
        print("\nCOMMENTS: all addressed")

    # --- Reviewers ---
    if wanted:
        print("\nREVIEWERS:")
        for name in wanted:
            status = check_reviewer_status(name, data)
            if status == "none":
                print(f"  [WAIT] {name}")
                has_wait = True
            elif status == "error":
                print(f"  [ERR]  {name}")
                has_actionable = True
            else:
                print(f"  [DONE] {name}")
    else:
        print("\nREVIEWERS: none triggered")

    print()
    if not has_wait and not has_actionable:
        print("ALL CLEAR")
        return StatusResult(exit_code=0, has_wait=False, has_actionable=False)
    return StatusResult(exit_code=1, has_wait=has_wait, has_actionable=has_actionable)


async def watch_status(
    owner: str,
    repo: str,
    pr: int,
    wanted: list[str] | None,
) -> int:
    loop = asyncio.get_running_loop()
    started_at = loop.time()
    attempt = 1
    while True:
        if attempt > 1:
            print(f"--- PR status attempt {attempt} ---")
        result = await check_status(owner, repo, pr, wanted)
        if result.exit_code == 0 or not result.wait_only:
            return result.exit_code

        elapsed = loop.time() - started_at
        if elapsed + WATCH_POLL_SECONDS > WATCH_MAX_WAIT_SECONDS:
            print(
                f"Timed out after {elapsed:.0f}s while PR had only wait-state blockers; "
                "run the watch command again when ready."
            )
            return 1

        print(f"WAIT_ONLY: rechecking in {WATCH_POLL_SECONDS:g}s inside pr_status.py.\n")
        await asyncio.sleep(WATCH_POLL_SECONDS)
        attempt += 1


def parse_reviewers(raw: str) -> list[str] | None:
    if raw == "auto":
        return None
    wanted = [r.strip().lower() for r in raw.split(",") if r.strip()]
    for r in wanted:
        if r not in REVIEWERS:
            print(
                f"error: unknown reviewer '{r}'. Known: {', '.join(REVIEWERS)}",
                file=sys.stderr,
            )
            raise SystemExit(2)
    return wanted


def main() -> int:
    parser = argparse.ArgumentParser(
        description="PR status check — CI + comments + reviewers report",
    )
    parser.add_argument("pr", type=int, help="PR number")
    parser.add_argument(
        "--reviewers",
        default="auto",
        help="Comma-separated reviewer names, or 'auto' to detect",
    )
    parser.add_argument(
        "--watch",
        action="store_true",
        help="Keep polling wait-only states until clear, actionable, or timed out",
    )
    args = parser.parse_args()

    wanted = parse_reviewers(args.reviewers)

    async def run() -> int:
        owner, repo = await detect_repo()
        if args.watch:
            return await watch_status(owner, repo, args.pr, wanted)
        return (await check_status(owner, repo, args.pr, wanted)).exit_code

    try:
        return asyncio.run(run())
    except RuntimeError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""PR status check — one-shot report of CI, comments, and reviewers.

Returns current PR state so the caller can decide what to do.
Run in a loop from the skill: check -> fix -> check -> fix -> done.

Exit codes:
  0 — All clear (CI green, no unaddressed comments, all reviewers responded)
  1 — Has issues (details in stdout)
"""

from __future__ import annotations

import argparse
import asyncio
import sys

from gh_async import (
    dedupe_check_runs,
    detect_repo,
    fetch_all,
    get_check_runs,
    get_pr_head_sha,
    run_gh,
)

# Cursor excluded: its review manifests as a CI check (cursor-bugbot)
REVIEWERS = {
    "claude": {"login": "github-actions[bot]"},
    "codex": {"login": "chatgpt-codex-connector[bot]"},
    "devin": {"login": "devin-ai-integration[bot]"},
}

TRIGGER_PATTERNS = {
    "codex": ["@codex review", "@codex r"],
}

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


def _is_error_response(body: str) -> bool:
    return any(pat in body.lower() for pat in ERROR_PATTERNS)


async def get_pr_author(pr: int) -> str:
    raw = await run_gh("pr", "view", str(pr), "--json", "author", "-q", ".author.login")
    return raw.strip()


def find_unaddressed_comments(data: dict, pr_author: str) -> list[dict]:
    """Find review comment threads not replied to by PR author."""
    review_comments = data.get("review_comments", [])
    threads: dict[int, list[dict]] = {}
    for c in review_comments:
        root_id = c.get("in_reply_to_id") or c["id"]
        threads.setdefault(root_id, []).append(c)

    unaddressed: list[dict] = []
    for root_id, thread in threads.items():
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
) -> int:
    pr_author = await get_pr_author(pr)
    if not pr_author:
        raise RuntimeError("Could not determine PR author")
    head_sha = await get_pr_head_sha(pr)

    data = await fetch_all(owner, repo, pr)
    runs = await get_check_runs(owner, repo, head_sha)

    if wanted is None:
        wanted = detect_triggered_reviewers(data)

    has_issues = False

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
                    has_issues = True
                else:
                    print(f"  [INFO] {name} (pending, non-blocking)")
            elif conclusion in ("success", "skipped", "neutral"):
                print(f"  [PASS] {name}")
            else:
                if required:
                    print(f"  [FAIL] {name}: {conclusion}")
                    has_issues = True
                else:
                    print(f"  [INFO] {name}: {conclusion} (non-blocking)")
    else:
        print("  (no checks detected)")

    # --- Unaddressed comments ---
    unaddressed = find_unaddressed_comments(data, pr_author)
    if unaddressed:
        has_issues = True
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
                has_issues = True
            elif status == "error":
                print(f"  [ERR]  {name}")
            else:
                print(f"  [DONE] {name}")
    else:
        print("\nREVIEWERS: none triggered")

    print()
    if not has_issues:
        print("ALL CLEAR")
    return 1 if has_issues else 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="PR status check — one-shot CI + comments + reviewers report",
    )
    parser.add_argument("pr", type=int, help="PR number")
    parser.add_argument(
        "--reviewers",
        default="auto",
        help="Comma-separated reviewer names, or 'auto' to detect",
    )
    args = parser.parse_args()

    wanted: list[str] | None = None
    if args.reviewers != "auto":
        wanted = [r.strip().lower() for r in args.reviewers.split(",") if r.strip()]
        for r in wanted:
            if r not in REVIEWERS:
                print(
                    f"error: unknown reviewer '{r}'. Known: {', '.join(REVIEWERS)}",
                    file=sys.stderr,
                )
                return 2

    owner, repo = detect_repo()

    try:
        return asyncio.run(check_status(owner, repo, args.pr, wanted))
    except RuntimeError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

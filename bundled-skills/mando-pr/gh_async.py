"""Async GitHub API helpers for pr_status.py.

Provides rate-limited gh CLI execution, concurrent data fetching,
and CI check run utilities.
"""

import asyncio
import json
import random
import subprocess
from datetime import datetime
from typing import Any

MAX_GH_RETRIES = 5
BASE_BACKOFF_SECONDS = 2


def _is_rate_limit_error(error: str) -> bool:
    """Detect both primary (429) and secondary (403) rate limits."""
    if "HTTP 429" in error:
        return True
    if "HTTP 403" in error and "rate limit" in error.lower():
        return True
    return False


async def run_gh(*args: str) -> str:
    """Run gh CLI with rate-limit retry (exponential backoff + jitter)."""
    delay = BASE_BACKOFF_SECONDS
    max_delay = BASE_BACKOFF_SECONDS * (2 ** (MAX_GH_RETRIES - 1))
    for attempt in range(1, MAX_GH_RETRIES + 1):
        proc = await asyncio.create_subprocess_exec(
            "gh",
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()
        if proc.returncode == 0:
            return stdout.decode()
        error = stderr.decode().strip() or "gh command failed"
        if not _is_rate_limit_error(error):
            raise RuntimeError(error)
        if attempt >= MAX_GH_RETRIES:
            raise RuntimeError(f"Rate limited after {MAX_GH_RETRIES} retries: {error}")
        jitter = random.uniform(0, delay)
        await asyncio.sleep(min(delay + jitter, max_delay))
        delay = min(delay * 2, max_delay)
    raise RuntimeError("unreachable")


def detect_repo() -> tuple[str, str]:
    result = subprocess.run(
        [
            "gh",
            "repo",
            "view",
            "--json",
            "owner,name",
            "-q",
            '.owner.login + "/" + .name',
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"detect_repo: gh failed (exit {result.returncode}): {result.stderr.strip()}"
        )
    raw = result.stdout.strip()
    if "/" in raw:
        return tuple(raw.split("/", 1))  # type: ignore[return-value]
    raise RuntimeError("Could not detect repo from git remote")


def parse_paginated(raw: str) -> list:
    """Parse gh api --paginate output (concatenated JSON arrays)."""
    if not raw:
        return []
    decoder = json.JSONDecoder()
    result: list = []
    pos = 0
    while pos < len(raw):
        stripped = raw[pos:].lstrip()
        if not stripped:
            break
        try:
            page, end = decoder.raw_decode(stripped)
        except json.JSONDecodeError as exc:
            raise json.JSONDecodeError(
                f"parse_paginated: malformed JSON at byte {pos} "
                f"({len(result)} items parsed so far): {exc.msg}",
                exc.doc,
                exc.pos,
            ) from exc
        if isinstance(page, list):
            result.extend(page)
        else:
            result.append(page)
        pos = len(raw) - len(stripped) + end
    return result


async def fetch_all(owner: str, repo: str, pr: int) -> dict[str, Any]:
    """Fetch comments, reviews, review comments, reactions in parallel."""

    async def paginated(endpoint: str) -> list:
        try:
            raw = await run_gh(
                "api",
                "--paginate",
                f"repos/{owner}/{repo}/{endpoint}?per_page=100",
            )
        except RuntimeError as exc:
            if "404" in str(exc):
                return []
            raise
        return parse_paginated(raw.strip())

    async def single(endpoint: str) -> list:
        try:
            raw = await run_gh("api", f"repos/{owner}/{repo}/{endpoint}")
        except RuntimeError as exc:
            if "404" in str(exc):
                return []
            raise
        if not raw.strip():
            return []
        parsed = json.loads(raw)
        if not isinstance(parsed, list):
            return []
        return parsed

    comments, reviews, review_comments, reactions = await asyncio.gather(
        paginated(f"issues/{pr}/comments"),
        paginated(f"pulls/{pr}/reviews"),
        paginated(f"pulls/{pr}/comments"),
        single(f"issues/{pr}/reactions"),
    )
    return {
        "comments": comments,
        "reviews": reviews,
        "review_comments": review_comments,
        "reactions": reactions,
    }


async def get_pr_head_sha(pr: int) -> str:
    raw = await run_gh(
        "pr",
        "view",
        str(pr),
        "--json",
        "headRefOid",
        "-q",
        ".headRefOid",
    )
    sha = raw.strip()
    if not sha:
        raise RuntimeError(f"get_pr_head_sha: empty response for PR #{pr}")
    return sha


async def get_check_runs(owner: str, repo: str, sha: str) -> list[dict]:
    try:
        raw = await run_gh(
            "api",
            f"repos/{owner}/{repo}/commits/{sha}/check-runs?per_page=100",
        )
    except RuntimeError as exc:
        if "404" in str(exc):
            return []
        raise
    if not raw.strip():
        return []
    payload = json.loads(raw)
    if not isinstance(payload, dict):
        return []
    return payload.get("check_runs", [])


def check_timestamp(run: dict) -> datetime | None:
    for key in ("completed_at", "started_at", "created_at"):
        if run.get(key):
            try:
                return datetime.fromisoformat(run[key].replace("Z", "+00:00"))
            except (ValueError, TypeError):
                continue
    return None


def dedupe_check_runs(runs: list[dict]) -> list[dict]:
    """Keep latest run per check name (handles CI re-runs)."""
    latest: dict[str, dict] = {}
    for run in runs:
        name = run.get("name") or "unknown"
        ts = check_timestamp(run)
        if name not in latest:
            latest[name] = run
        elif ts:
            existing_ts = check_timestamp(latest[name])
            if existing_ts is None or ts > existing_ts:
                latest[name] = run
    return list(latest.values())

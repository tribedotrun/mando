"""Async GitHub API helpers for pr_status.py.

Provides rate-limited gh CLI execution, concurrent data fetching,
and CI check run utilities.
"""

import asyncio
import json
import random
from datetime import datetime
from typing import Any

MAX_GH_RETRIES = 5
BASE_BACKOFF_SECONDS = 2


class GhError(RuntimeError):
    """Raised when the `gh` CLI exits non-zero.

    Carries the structured HTTP status (when gh reports one in stderr) so
    callers can branch on e.g. 404 without substring-matching formatted
    messages.
    """

    def __init__(self, stderr: str, http_status: int | None = None) -> None:
        super().__init__(stderr)
        self.stderr = stderr
        self.http_status = http_status

    def is_not_found(self) -> bool:
        return self.http_status == 404


def _parse_http_status(stderr: str) -> int | None:
    """Extract the HTTP status gh reports on API failures, e.g. `HTTP 404:`."""
    marker = "HTTP "
    idx = stderr.find(marker)
    if idx == -1:
        return None
    digits: list[str] = []
    for ch in stderr[idx + len(marker):]:
        if ch.isdigit():
            digits.append(ch)
            if len(digits) == 3:
                break
        else:
            break
    if len(digits) != 3:
        return None
    try:
        return int("".join(digits))
    except ValueError:
        return None


def _is_rate_limit_error(error: str, http_status: int | None) -> bool:
    """Detect both primary (429) and secondary (403) rate limits."""
    if http_status == 429:
        return True
    if http_status == 403 and "rate limit" in error.lower():
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
        http_status = _parse_http_status(error)
        if not _is_rate_limit_error(error, http_status):
            raise GhError(error, http_status=http_status)
        if attempt >= MAX_GH_RETRIES:
            raise GhError(
                f"Rate limited after {MAX_GH_RETRIES} retries: {error}",
                http_status=http_status,
            )
        jitter = random.uniform(0, delay)
        await asyncio.sleep(min(delay + jitter, max_delay))
        delay = min(delay * 2, max_delay)
    raise RuntimeError("unreachable")


async def detect_repo() -> tuple[str, str]:
    raw = (
        await run_gh(
            "repo",
            "view",
            "--json",
            "owner,name",
            "-q",
            '.owner.login + "/" + .name',
        )
    ).strip()
    if "/" in raw:
        owner, name = raw.split("/", 1)
        return owner, name
    raise RuntimeError("Could not detect repo from git remote")


async def _paginated_items(endpoint: str) -> list:
    """Fetch a paginated REST endpoint as a flat list of items.

    Uses `gh api --paginate --jq '.[]'` so each page's array is streamed as
    newline-delimited JSON values, one item per line.
    """
    try:
        raw = await run_gh("api", "--paginate", "--jq", ".[]", endpoint)
    except GhError as exc:
        if exc.is_not_found():
            return []
        raise
    items: list = []
    for line in raw.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        items.append(json.loads(stripped))
    return items


async def _paginated_object_items(endpoint: str, key: str) -> list:
    """Flatten a paginated REST response whose items live under ``key``."""
    try:
        raw = await run_gh("api", "--paginate", "--jq", f".{key}[]", endpoint)
    except GhError as exc:
        if exc.is_not_found():
            return []
        raise
    return [json.loads(line) for line in raw.splitlines() if line.strip()]


_THREAD_QUERY = """
query($owner: String!, $repo: String!, $pr: Int!, $cursor: String) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $pr) {
      reviewThreads(first: 100, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes {
          isResolved
          isOutdated
          comments(first: 1) { nodes { databaseId } }
        }
      }
    }
  }
}
"""


async def fetch_review_threads_meta(
    owner: str, repo: str, pr: int
) -> dict[int, dict[str, bool]]:
    """Per-thread `isResolved` / `isOutdated`, keyed by the root comment's databaseId.

    Returns an empty map if the PR or repo cannot be reached; missing metadata
    falls back to "needs reply" in the caller.
    """
    meta: dict[int, dict[str, bool]] = {}
    cursor: str | None = None
    while True:
        args = [
            "api",
            "graphql",
            "-f",
            f"query={_THREAD_QUERY}",
            "-f",
            f"owner={owner}",
            "-f",
            f"repo={repo}",
            "-F",
            f"pr={pr}",
        ]
        if cursor:
            args.extend(["-f", f"cursor={cursor}"])
        try:
            raw = await run_gh(*args)
        except GhError as exc:
            if exc.is_not_found():
                return meta
            raise
        payload = json.loads(raw)
        review_threads = (
            payload.get("data", {})
            .get("repository", {})
            .get("pullRequest", {})
            .get("reviewThreads", {})
        ) or {}
        for thread in review_threads.get("nodes") or []:
            comments = (thread.get("comments") or {}).get("nodes") or []
            if not comments:
                continue
            root_id = comments[0].get("databaseId")
            if root_id is None:
                continue
            meta[int(root_id)] = {
                "is_resolved": bool(thread.get("isResolved")),
                "is_outdated": bool(thread.get("isOutdated")),
            }
        page = review_threads.get("pageInfo") or {}
        if not page.get("hasNextPage"):
            break
        next_cursor = page.get("endCursor")
        if not next_cursor or next_cursor == cursor:
            break
        cursor = next_cursor
    return meta


async def fetch_all(owner: str, repo: str, pr: int) -> dict[str, Any]:
    """Fetch comments, reviews, review comments, and thread metadata in parallel."""
    comments, reviews, review_comments, thread_meta = await asyncio.gather(
        _paginated_items(f"repos/{owner}/{repo}/issues/{pr}/comments?per_page=100"),
        _paginated_items(f"repos/{owner}/{repo}/pulls/{pr}/reviews?per_page=100"),
        _paginated_items(f"repos/{owner}/{repo}/pulls/{pr}/comments?per_page=100"),
        fetch_review_threads_meta(owner, repo, pr),
    )
    return {
        "comments": comments,
        "reviews": reviews,
        "review_comments": review_comments,
        "thread_meta": thread_meta,
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


_PR_HEAD_QUERY = """
query($owner: String!, $repo: String!, $pr: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $pr) {
      headRefOid
    }
  }
}
"""


async def get_pr_head_info(owner: str, repo: str, pr: int) -> dict[str, str | None]:
    """Return the PR head SHA.

    GitHub does not expose a reliable PR head-ref update timestamp here.
    ``pushed_at`` remains as ``None`` for caller compatibility; review windows
    use an explicit timestamp or the watcher's first observation instead.
    """
    raw = await run_gh(
        "api",
        "graphql",
        "-f",
        f"query={_PR_HEAD_QUERY}",
        "-f",
        f"owner={owner}",
        "-f",
        f"repo={repo}",
        "-F",
        f"pr={pr}",
    )
    payload = json.loads(raw)
    pull = (
        payload.get("data", {})
        .get("repository", {})
        .get("pullRequest")
    ) or {}
    sha = (pull.get("headRefOid") or "").strip()
    if not sha:
        raise RuntimeError(f"get_pr_head_info: empty head SHA for PR #{pr}")
    return {"sha": sha, "pushed_at": None}


async def get_workflow_runs(owner: str, repo: str, run_ids: list[int]) -> list[dict]:
    """Fetch explicitly requested Actions workflow runs concurrently."""
    if not run_ids:
        return []

    async def fetch(run_id: int) -> dict:
        raw = await run_gh("api", f"repos/{owner}/{repo}/actions/runs/{run_id}")
        payload = json.loads(raw)
        if not isinstance(payload, dict):
            raise RuntimeError(f"workflow run {run_id}: invalid GitHub response")
        return payload

    return list(await asyncio.gather(*(fetch(run_id) for run_id in run_ids)))


async def get_check_runs(owner: str, repo: str, sha: str) -> list[dict]:
    return await _paginated_object_items(
        f"repos/{owner}/{repo}/commits/{sha}/check-runs?per_page=100",
        "check_runs",
    )


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

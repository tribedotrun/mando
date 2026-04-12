#!/usr/bin/env python3
"""
Scout summary/article backfill — one-shot heal from legacy file layout.

Before PR #727, scout summaries and articles lived on disk keyed by
`{db_id}-{slug}.md` and `{db_id}-article.md`. After the migration they live
in scout_items.summary / scout_items.article. This script copies the
existing file content into the new columns for rows that are still NULL.

The summary match is by SLUG ONLY — the filename's ID prefix is ignored,
which is exactly what heals the drift that caused the empty-card bug
(item #4 pointing at file `007-anthropics-managed-agents...md`).

Usage:
    python3 scripts/scout-backfill.py <data_dir>

Where <data_dir> is the Mando data directory, for example:
    ~/.mando          (prod)
    ~/.mando-dev      (dev)
    .dev/sandbox/*    (sandbox)

The script is idempotent: a second run with no newly-stranded items
reports `healed=0` and exits.
"""

from __future__ import annotations

import os
import re
import sqlite3
import sys
from pathlib import Path


SLUG_MAX_LEN = 60


def slugify_title(title: str) -> str:
    """Match the Rust slugify_title behavior byte-for-byte.

    Lowercase, strip non-alphanumerics except hyphens, treat whitespace +
    underscore as hyphens, collapse consecutive hyphens, trim, truncate to 60.
    """
    lower = title.lower()
    out_chars: list[str] = []
    for ch in lower:
        if ch.isascii() and (ch.isalnum() or ch == "-"):
            out_chars.append(ch)
        elif ch.isspace() or ch == "_":
            out_chars.append("-")
    collapsed: list[str] = []
    prev_hyphen = False
    for ch in out_chars:
        if ch == "-":
            if not prev_hyphen:
                collapsed.append("-")
            prev_hyphen = True
        else:
            collapsed.append(ch)
            prev_hyphen = False
    trimmed = "".join(collapsed).strip("-")
    if len(trimmed) <= SLUG_MAX_LEN:
        return trimmed
    return trimmed[:SLUG_MAX_LEN].rstrip("-")


SUMMARY_NAME_RE = re.compile(r"^(?P<id>\d+)-(?P<slug>.+)\.md$")


def index_summaries(summaries_dir: Path) -> dict[str, Path]:
    """Scan summaries dir and return {slug: path}. Ignores the ID prefix.

    If two files share the same slug, keep the one with the higher numeric ID
    prefix — in the drift scenario that motivated this script, the more recent
    processing run (higher ID) is more likely to reflect the current content.
    Tiebreaker is lexicographic filename order. Handles any ID length, not
    just three-digit prefixes (future-proof past item #999).
    """
    result: dict[str, tuple[int, Path]] = {}
    if not summaries_dir.exists():
        return {}
    for entry in sorted(summaries_dir.iterdir()):
        if not entry.is_file():
            continue
        match = SUMMARY_NAME_RE.match(entry.name)
        if not match:
            continue
        slug = match.group("slug")
        file_id = int(match.group("id"))
        existing = result.get(slug)
        if existing is None or file_id > existing[0]:
            result[slug] = (file_id, entry)
    return {slug: path for slug, (_id, path) in result.items()}


def backfill(data_dir: Path) -> int:
    db_path = data_dir / "mando.db"
    if not db_path.exists():
        print(f"error: database not found at {db_path}", file=sys.stderr)
        return 2

    scout_dir = data_dir / "scout"
    summaries_dir = scout_dir / "summaries"
    content_dir = scout_dir / "content"

    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    try:
        cur = conn.execute(
            """
            SELECT id, title, summary, article, status
            FROM scout_items
            WHERE status IN ('processed', 'saved', 'archived')
              AND (summary IS NULL OR article IS NULL)
            ORDER BY id
            """
        )
        stranded = cur.fetchall()

        if not stranded:
            print("nothing to do: all processed/saved/archived items already "
                  "have summary and article populated")
            return 0

        print(f"found {len(stranded)} item(s) needing heal")

        summary_index = index_summaries(summaries_dir)
        print(f"summary files on disk: {len(summary_index)} (indexed by slug)")

        summary_healed = 0
        summary_missed = 0
        summary_errors = 0
        article_healed = 0
        article_missed = 0
        article_errors = 0
        matched_slugs: set[str] = set()

        for row in stranded:
            item_id = row["id"]
            title = row["title"]
            needs_summary = row["summary"] is None
            needs_article = row["article"] is None

            # Per-item try/except + per-item commit: a bad file or a transient
            # SQLite error must not poison the whole batch, and observed stdout
            # progress must match persisted DB state. Counter increments are
            # deferred until after conn.commit() so a rollback never desynchronises
            # the running totals from what is actually persisted.
            did_heal_summary = False
            did_heal_article = False
            healed_slug: str | None = None
            item_summary_missed = False
            item_article_missed = False

            try:
                if needs_summary:
                    if not title:
                        print(f"  #{item_id}: skip summary (no title)")
                        item_summary_missed = True
                    else:
                        slug = slugify_title(title)
                        file_path = summary_index.get(slug)
                        if file_path is None:
                            print(f"  #{item_id}: summary slug '{slug}' not found")
                            item_summary_missed = True
                        else:
                            content = file_path.read_text(encoding="utf-8")
                            conn.execute(
                                "UPDATE scout_items SET summary = ?, rev = rev + 1 WHERE id = ?",
                                (content, item_id),
                            )
                            did_heal_summary = True
                            healed_slug = slug
                            print(f"  #{item_id}: summary <- {file_path.name}")

                if needs_article:
                    article_path = content_dir / f"{item_id:03d}-article.md"
                    if article_path.exists():
                        content = article_path.read_text(encoding="utf-8")
                        conn.execute(
                            "UPDATE scout_items SET article = ?, rev = rev + 1 WHERE id = ?",
                            (content, item_id),
                        )
                        did_heal_article = True
                        print(f"  #{item_id}: article <- {article_path.name}")
                    else:
                        item_article_missed = True
                        print(f"  #{item_id}: article file missing ({article_path.name})")

                conn.commit()

                # Counters only after successful commit.
                if did_heal_summary:
                    summary_healed += 1
                    matched_slugs.add(healed_slug)
                if item_summary_missed:
                    summary_missed += 1
                if did_heal_article:
                    article_healed += 1
                if item_article_missed:
                    article_missed += 1
            except Exception as e:
                conn.rollback()
                if needs_summary:
                    summary_errors += 1
                if needs_article:
                    article_errors += 1
                print(
                    f"  #{item_id}: error ({type(e).__name__}: {e}) — item skipped, continuing",
                    file=sys.stderr,
                )

        orphan_slugs = [s for s in summary_index if s not in matched_slugs]
        orphan_slugs.sort()

        print("")
        print("=" * 60)
        print(f"healed:  summary={summary_healed}  article={article_healed}")
        print(f"missed:  summary={summary_missed}  article={article_missed}")
        print(f"errors:  summary={summary_errors}  article={article_errors}")
        print(f"orphans: {len(orphan_slugs)} summary file(s) without a DB match")
        if orphan_slugs:
            for slug in orphan_slugs[:20]:
                print(f"  - {slug}")
            if len(orphan_slugs) > 20:
                print(f"  ... and {len(orphan_slugs) - 20} more")
        print("=" * 60)
        print("orphan files are NOT deleted; clean up manually if wanted")
        return 1 if (summary_errors or article_errors) else 0
    finally:
        conn.close()


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    raw = sys.argv[1]
    data_dir = Path(os.path.expanduser(raw)).resolve()
    if not data_dir.is_dir():
        print(f"error: {data_dir} is not a directory", file=sys.stderr)
        return 2
    return backfill(data_dir)


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Validate that a GitHub PR body matches the mando-pr-summary contract."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


REQUIRED_HEADINGS = [
    "## Problem",
    "## Solution",
    "## Code Diff",
    "## Evidence",
    "## Reviewer Checklist",
    "## Testing & Verification",
]

REQUIRED_TESTING_HEADINGS = [
    "### Unit tests",
    "### E2E regression",
    "### E2E verification",
]

ENV_VAR_CHECKLIST_PATTERN = re.compile(
    r"^\s*\d+\.\s+\[[ xX]\]\s+\*\*Env vars\*\*:\s*(?P<detail>.+)$",
    re.MULTILINE,
)

REVIEWER_CHECKLIST_ITEM_PATTERN = re.compile(
    r"^\s*\d+\.\s+\[[ xX]\]\s+\*\*(?P<label>[^*]+)\*\*:\s*(?P<detail>.*)$",
    re.MULTILINE,
)

REQUIRED_REVIEWER_CHECKLIST_LABELS = [
    "UI copy",
    "Architecture surface",
]

ENV_VAR_RATIONALE_PATTERN = re.compile(
    r"\b("
    r"secret|credential|per-deployment|per-deploy|deployment-specific|"
    r"operator|runtime setting|machine-local|environment boundary|"
    r"config field|constant|variable"
    r")\b",
    re.IGNORECASE,
)


def read_body(args: argparse.Namespace) -> str:
    if args.pr:
        result = subprocess.run(
            ["gh", "pr", "view", str(args.pr), "--json", "body", "--jq", ".body"],
            check=True,
            text=True,
            capture_output=True,
        )
        return result.stdout

    if args.body_file:
        return Path(args.body_file).read_text()

    if not sys.stdin.isatty():
        return sys.stdin.read()

    raise SystemExit("Provide --pr, --body-file, or pipe a PR body on stdin.")


def heading_index(body: str, heading: str) -> int:
    pattern = re.compile(rf"^{re.escape(heading)}\s*$", re.MULTILINE)
    match = pattern.search(body)
    return -1 if match is None else match.start()


def section_text(body: str, heading: str) -> str:
    start = heading_index(body, heading)
    if start == -1:
        return ""

    after_heading = body.find("\n", start)
    if after_heading == -1:
        return ""

    next_heading = re.search(r"^##\s+", body[after_heading + 1 :], re.MULTILINE)
    if next_heading is None:
        return body[after_heading + 1 :]

    return body[after_heading + 1 : after_heading + 1 + next_heading.start()]


def validate(body: str) -> list[str]:
    errors: list[str] = []
    stripped = body.lstrip()

    if re.match(r"#\s+PR\s+#?\d+\s+Summary\b", stripped):
        errors.append(
            "body starts with the persisted work-summary artifact title, not the canonical PR body"
        )

    forbidden_headings = ["## Solution Diagram", "## What changed"]
    for heading in forbidden_headings:
        if heading_index(body, heading) != -1:
            errors.append(f"body contains artifact-only heading: {heading}")

    section_positions: list[tuple[str, int]] = []
    for heading in REQUIRED_HEADINGS:
        index = heading_index(body, heading)
        if index == -1:
            errors.append(f"missing required heading: {heading}")
        section_positions.append((heading, index))

    present_positions = [index for _, index in section_positions if index != -1]
    if present_positions != sorted(present_positions):
        errors.append("canonical sections are out of order")

    for heading in REQUIRED_TESTING_HEADINGS:
        if heading_index(body, heading) == -1:
            errors.append(f"missing required testing heading: {heading}")

    if not re.search(r"^\*\*What changed\*\*:\s+\S", body, re.MULTILINE):
        errors.append("missing inline '**What changed**:' sentence in Solution")

    reviewer_checklist = section_text(body, "## Reviewer Checklist")
    env_vars_match = ENV_VAR_CHECKLIST_PATTERN.search(reviewer_checklist)
    if env_vars_match is None:
        errors.append("Reviewer Checklist is missing the '**Env vars**' audit item")
    else:
        env_vars_detail = env_vars_match.group("detail").strip()
        if env_vars_detail.startswith("<") and env_vars_detail.endswith(">"):
            errors.append("Env vars checklist item still contains placeholder text")
        elif not re.match(
            r"^none\b", env_vars_detail, re.IGNORECASE
        ) and not ENV_VAR_RATIONALE_PATTERN.search(env_vars_detail):
            errors.append(
                "Env vars item needs 'none' or an env-boundary rationale"
            )

    checklist_items = {
        match.group("label").strip(): match.group("detail").strip()
        for match in REVIEWER_CHECKLIST_ITEM_PATTERN.finditer(reviewer_checklist)
    }
    for label in REQUIRED_REVIEWER_CHECKLIST_LABELS:
        detail = checklist_items.get(label)
        if detail is None:
            errors.append(f"Reviewer Checklist is missing the '**{label}**' item")
        elif not detail:
            errors.append(f"Reviewer Checklist '**{label}**' item is empty")
        elif detail.startswith("<") and detail.endswith(">"):
            errors.append(
                f"Reviewer Checklist '**{label}**' item still contains placeholder text"
            )

    code_diff = heading_index(body, "## Code Diff")
    solution = heading_index(body, "## Solution")
    if code_diff != -1 and solution != -1 and code_diff < solution:
        errors.append("Code Diff appears before Solution, which matches the artifact shape")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pr", help="GitHub PR number or URL to validate")
    parser.add_argument("--body-file", help="Local markdown file to validate")
    args = parser.parse_args()

    body = read_body(args)
    errors = validate(body)
    if errors:
        print("PR body validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("PR body validation passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

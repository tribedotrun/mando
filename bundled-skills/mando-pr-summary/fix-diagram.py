#!/usr/bin/env python3
"""
Fix right-border alignment in ASCII box diagrams.
Ensures every │ line within a ┌─┐ container aligns with the ┐ column.

Approach:
1. Find all boxes (┌─┐ ... └─┘ pairs) and their column boundaries.
2. Process innermost boxes first (so inner borders are fixed before outer).
3. For each content line, find the misplaced │ border and relocate it.
4. After every single-box fix, re-scan box coordinates from the current
   line buffer. Fixing one sibling can shift the cached coordinates of
   another; re-scanning each pass means we always work from fresh
   positions instead of stale ones.

Shift handling:
- Border too far RIGHT (content too wide): don't compensate — trimming content
  naturally left-shifts all subsequent characters, fixing cascaded misalignment.
- Border too far LEFT (content too narrow): absorb leading spaces from "after"
  to prevent line growth that would push subsequent borders right.

Usage: python3 fix-diagram.py <<'EOF'
       <diagram>
       EOF
   or: python3 fix-diagram.py < diagram.txt
"""

import re
import sys


def find_matching_close(lines, start, left_col):
    """Find the └ line index that matches a ┌ at (start, left_col)."""
    depth = 0
    for i in range(start, len(lines)):
        for m in re.finditer("┌", lines[i]):
            if m.start() == left_col:
                depth += 1
        for m in re.finditer("└", lines[i]):
            if m.start() == left_col:
                depth -= 1
                if depth == 0:
                    return i
    return None


def build_exclude_cols(box_idx, top, bot, left_col, right_col, boxes):
    """Build set of columns to exclude when searching for this box's border.

    Excludes borders of:
    - PARENT boxes (contain this box) — prevents grabbing outer │ for inner box
    - NESTED boxes (inside this box) — already fixed, their borders are correct
    """
    exclude = set()
    for bidx2, (t2, b2, l2, r2) in enumerate(boxes):
        if bidx2 == box_idx:
            continue
        is_parent = t2 <= top and b2 >= bot and l2 < left_col and r2 > right_col
        is_nested = t2 >= top and b2 <= bot and l2 > left_col and r2 < right_col
        if is_parent or is_nested:
            exclude.add(l2)
            exclude.add(r2)
    return exclude


def fix_box_content(lines, top, bot, left_col, right_col, exclude_cols):
    """Fix content lines for a single box, relocating misplaced │ borders."""
    target_w = right_col - left_col - 1

    for k in range(top + 1, bot):
        line = lines[k]

        # Skip lines without │ at left_col (nested ┌/└ lines, arrows, etc.)
        if left_col >= len(line) or line[left_col] != "│":
            continue

        # Pad line to at least right_col + 1 chars
        if len(line) <= right_col:
            line = line + " " * (right_col + 1 - len(line))

        # Already correct
        if line[right_col] == "│":
            lines[k] = line
            continue

        # Search outward from right_col for the misplaced right border
        misplaced = None
        for offset in range(1, len(line)):
            for candidate in [right_col - offset, right_col + offset]:
                if candidate <= left_col or candidate >= len(line):
                    continue
                if line[candidate] == "│" and candidate not in exclude_cols:
                    misplaced = candidate
                    break
            if misplaced is not None:
                break

        if misplaced is not None:
            # Relocate border from misplaced to right_col
            content = line[left_col + 1 : misplaced]

            # Pad/trim content to target width
            cs = content.rstrip()
            if len(cs) <= target_w:
                padded = cs + " " * (target_w - len(cs))
            else:
                padded = cs[:target_w]

            if misplaced > right_col:
                # Border was too far right (content too wide).
                # Trimming content naturally shifts everything left — no compensation.
                after = line[misplaced + 1 :]
            else:
                # Border was too far left (content too narrow).
                # Absorb leading spaces from after to keep subsequent borders in place.
                after = line[misplaced + 1 :]
                shift = right_col - misplaced
                consumed, idx = 0, 0
                while idx < len(after) and after[idx] == " " and consumed < shift:
                    idx += 1
                    consumed += 1
                after = after[idx:]

            lines[k] = line[: left_col + 1] + padded + "│" + after
        else:
            # No misplaced border found — insert │ at right_col
            before_content = line[left_col + 1 : right_col]
            after_content = line[right_col + 1 :] if right_col + 1 <= len(line) else ""

            bs = before_content.rstrip()
            if len(bs) <= target_w:
                padded = bs + " " * (target_w - len(bs))
            else:
                padded = bs[:target_w]

            lines[k] = line[: left_col + 1] + padded + "│" + after_content


def fix_close_line(lines, bot, left_col, right_col):
    """Fix the └─┘ closing line to match the box width."""
    line = lines[bot]
    if left_col >= len(line) or line[left_col] != "└":
        return

    # Pad if needed
    if len(line) <= right_col:
        line = line + " " * (right_col + 1 - len(line))

    # Already correct
    if line[right_col] == "┘":
        lines[bot] = line
        return

    # Search outward from right_col for misplaced ┘
    close_pos = None
    for offset in range(1, len(line)):
        for candidate in [right_col - offset, right_col + offset]:
            if candidate <= left_col or candidate >= len(line):
                continue
            if line[candidate] == "┘":
                close_pos = candidate
                break
        if close_pos is not None:
            break

    if close_pos is None:
        # No ┘ found nearby — insert one at right_col
        lines[bot] = line[:right_col] + "┘" + line[right_col + 1 :]
        return

    # Relocate ┘ from close_pos to right_col
    before = line[:left_col]
    inner = line[left_col + 1 : close_pos]
    target_w = right_col - left_col - 1

    # Pad/trim inner with ─
    inner_stripped = inner.rstrip("─")
    if len(inner_stripped) < target_w:
        inner = inner_stripped + "─" * (target_w - len(inner_stripped))
    else:
        inner = inner_stripped[:target_w]

    if close_pos > right_col:
        after = line[close_pos + 1 :]
    else:
        after = line[close_pos + 1 :]
        shift = right_col - close_pos
        consumed, idx = 0, 0
        while idx < len(after) and after[idx] == " " and consumed < shift:
            idx += 1
            consumed += 1
        after = after[idx:]

    lines[bot] = before + "└" + inner + "┘" + after


def is_structurally_correct(box, lines):
    """A box is correct when every │/└ on its left_col line up with │/┘ on right_col."""
    top, bot, left_col, right_col = box
    for k in range(top + 1, bot):
        line = lines[k]
        # Skip rows where the box's left border isn't present (arrows, gaps, etc.)
        if left_col >= len(line) or line[left_col] != "│":
            continue
        if len(line) <= right_col or line[right_col] != "│":
            return False
    bot_line = lines[bot]
    if left_col >= len(bot_line) or bot_line[left_col] != "└":
        return False
    if len(bot_line) <= right_col or bot_line[right_col] != "┘":
        return False
    return True


def _scan_boxes(lines, warn_unmatched=False):
    """Return the list of (top, bot, left_col, right_col) boxes in the current buffer.

    Emits a stderr warning for every ┌─+┐ that has no matching └ at the same column
    when `warn_unmatched` is True.
    """
    boxes = []
    for i, line in enumerate(lines):
        for m in re.finditer(r"┌─+┐", line):
            left_col = m.start()
            right_col = m.end() - 1
            bot = find_matching_close(lines, i, left_col)
            if bot is not None:
                boxes.append((i, bot, left_col, right_col))
            elif warn_unmatched:
                print(
                    f"fix-diagram: warning: unmatched ┌ at line {i + 1}, col {left_col + 1} "
                    "(no matching └ found at the same column)",
                    file=sys.stderr,
                )
    return boxes


def fix_diagram(text):
    lines = text.split("\n")

    # One-shot warning pass for unmatched ┌. Fixes never relocate ┌/└ characters,
    # so an unmatched opener in the original input stays unmatched.
    _scan_boxes(lines, warn_unmatched=True)

    # Each iteration fixes one innermost still-broken box, then re-scans so the
    # next iteration sees any column shifts caused by the prior fix.
    max_iterations = len(lines) * 4 + 16
    for _ in range(max_iterations):
        boxes = _scan_boxes(lines)
        unfixed = [b for b in boxes if not is_structurally_correct(b, lines)]
        if not unfixed:
            break
        unfixed.sort(key=lambda b: b[1] - b[0])
        target = unfixed[0]
        target_idx = boxes.index(target)
        exclude_cols = build_exclude_cols(
            target_idx, target[0], target[1], target[2], target[3], boxes
        )
        fix_box_content(lines, target[0], target[1], target[2], target[3], exclude_cols)
        fix_close_line(lines, target[1], target[2], target[3])

    return "\n".join(line.rstrip() for line in lines)


if __name__ == "__main__":
    text = sys.stdin.read()
    result = fix_diagram(text)
    print(result)

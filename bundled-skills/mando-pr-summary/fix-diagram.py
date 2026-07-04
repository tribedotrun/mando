#!/usr/bin/env python3
# Also used by the standalone AI Toolkit x-diagram skill; keep behavior in sync when copying updates.
"""
Fix right-border alignment in ASCII box diagrams.
Ensures every │ line within a ┌─┐ container aligns with the ┐ column.

Approach:
1. Find all boxes (┌─┐ ... └─┘ pairs) and their column boundaries.
2. Fix one innermost still-broken box per pass, then re-scan box coordinates
   from the current line buffer. Fixing one sibling can shift the cached
   coordinates of another; re-scanning each pass means we always work from
   fresh positions instead of stale ones.
3. Pre-scan each box: if any content border is past the ┐ column, expand the
   header first, then re-scan.
4. For each content line, find the misplaced │ border and relocate it.
5. Exclude only PARENT and NESTED box borders during search — not siblings.
6. Interior │ characters (state machine arrows) vs borders:
   - Within BORDER_TOLERANCE of expected position: accepted (probably border)
   - Far away: must pass is_likely_border (no text content after it)

Shift handling:
- Border too far RIGHT (content too wide): don't compensate — trimming content
  naturally left-shifts all subsequent characters, fixing cascaded misalignment.
- Border too far LEFT (content too narrow): absorb leading spaces from "after"
  to prevent line growth that would push subsequent borders right.

Usage: echo '<diagram>' | python3 fix-diagram.py
   or: python3 fix-diagram.py < diagram.txt
"""

import re
import sys

# A │ within this many chars of the expected right_col is accepted without
# content checking. Beyond this distance, is_likely_border is required.
BORDER_TOLERANCE = 3


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

    Does NOT exclude SIBLING boxes — their borders may be at cascaded positions
    that overlap with our misplaced border's actual location.
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


def is_likely_border(line, pos):
    """Check if │ at pos looks like a right border, not interior content.

    A right-border │ has only whitespace and box-drawing characters after it.
    An interior │ (state machine arrow, tree branch) has text content after it.
    """
    BOX_CHARS = set("│┌┐└┘─┼┤├┬┴")
    for j in range(pos + 1, len(line)):
        ch = line[j]
        if ch == " ":
            continue
        if ch in BOX_CHARS:
            continue
        return False
    return True


def find_right_border(line, left_col, right_col, exclude_cols):
    """Search outward from right_col for a misplaced │ border.

    Within BORDER_TOLERANCE of right_col, any │ (not excluded) is accepted.
    Beyond that, is_likely_border is required to avoid grabbing interior arrows.
    Returns the column position, or None if not found.
    """
    for offset in range(1, len(line)):
        for candidate in [right_col - offset, right_col + offset]:
            if candidate <= left_col or candidate >= len(line):
                continue
            if line[candidate] != "│" or candidate in exclude_cols:
                continue
            if (
                abs(candidate - right_col) <= BORDER_TOLERANCE
                or is_likely_border(line, candidate)
            ):
                return candidate
    return None


def expand_header(lines, top, left_col, old_right, new_right):
    """Expand ┌─┐ header from old_right to new_right."""
    header = lines[top]
    new_inner_w = new_right - left_col - 1
    after = header[old_right + 1 :] if old_right + 1 < len(header) else ""
    lines[top] = header[:left_col] + "┌" + "─" * new_inner_w + "┐" + after


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

        misplaced = find_right_border(line, left_col, right_col, exclude_cols)

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
                # Trimming content naturally shifts everything left - no compensation.
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
            # No misplaced border found - insert │ at right_col
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
        # No ┘ found nearby - insert one at right_col
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


def align_interior_verticals(lines, top, bot, left_col, right_col):
    """Align interior │ that form vertical runs within a box.

    Detects │ characters at ±1 column on adjacent lines (same vertical flow
    line, slightly misaligned) and aligns them to the majority column by
    swapping with the adjacent space.

    Uses connected-component analysis: two │ cells are connected if they're
    on adjacent lines and within ±1 column. This prevents merging unrelated
    vertical lines that happen to be at similar columns but are separated by
    blank lines.
    """
    # Collect all interior │ positions (excluding box borders)
    cells = set()
    for k in range(top + 1, bot):
        line = lines[k]
        for j in range(left_col + 1, min(right_col, len(line))):
            if line[j] == "│":
                cells.add((k, j))

    if not cells:
        return

    # Build connected components via BFS
    visited = set()
    components = []
    for cell in cells:
        if cell in visited:
            continue
        component = []
        queue = [cell]
        visited.add(cell)
        while queue:
            ck, cj = queue.pop(0)
            component.append((ck, cj))
            for dk in [-1, 1]:
                for dj in [-1, 0, 1]:
                    n = (ck + dk, cj + dj)
                    if n in cells and n not in visited:
                        visited.add(n)
                        queue.append(n)
        components.append(component)

    # For each component with ±1 column spread, align to majority
    for component in components:
        cols = [c for _, c in component]
        unique_cols = set(cols)
        if len(unique_cols) <= 1:
            continue
        # Only fix ±1 drift - larger spreads are intentional structure
        if max(unique_cols) - min(unique_cols) > 1:
            continue

        col_counts = {}
        for c in cols:
            col_counts[c] = col_counts.get(c, 0) + 1
        target = max(col_counts, key=col_counts.get)

        for k, j in component:
            if j == target:
                continue
            line = lines[k]
            if target < len(line) and line[target] == " ":
                char_list = list(line)
                char_list[j] = " "
                char_list[target] = "│"
                lines[k] = "".join(char_list)


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
        top, bot, left_col, right_col = target
        exclude_cols = build_exclude_cols(
            target_idx, top, bot, left_col, right_col, boxes
        )

        # Pre-scan: check if any content line has a border past right_col.
        # If so, the ┌─┐ header is too narrow — expand it and re-scan so all
        # box coordinates (including shifted siblings) are picked up fresh.
        max_right = right_col
        for k in range(top + 1, bot):
            line = lines[k]
            if left_col >= len(line) or line[left_col] != "│":
                continue
            padded = (
                line
                if len(line) > right_col
                else line + " " * (right_col + 1 - len(line))
            )
            if padded[right_col] == "│":
                continue
            pos = find_right_border(padded, left_col, right_col, exclude_cols)
            if pos is not None and pos > max_right:
                max_right = pos

        if max_right > right_col:
            expand_header(lines, top, left_col, right_col, max_right)
            continue

        fix_box_content(lines, top, bot, left_col, right_col, exclude_cols)
        fix_close_line(lines, bot, left_col, right_col)

    # Borders are settled — align interior │ runs (state-machine arrows etc.),
    # innermost boxes first.
    for top, bot, left_col, right_col in sorted(
        _scan_boxes(lines), key=lambda b: b[1] - b[0]
    ):
        align_interior_verticals(lines, top, bot, left_col, right_col)

    return "\n".join(line.rstrip() for line in lines)


if __name__ == "__main__":
    text = sys.stdin.read()
    result = fix_diagram(text)
    print(result)

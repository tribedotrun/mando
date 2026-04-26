// Locks the three terminal-resume bugs fixed in PR #983:
//
// 1. Re-resume the same session id: relies on the orchestration's reactive
//    effect (not exercised here — pure JSX, requires a runner). Instead we
//    pin the underlying invariants below.
// 2. Resuming a second session must not evict the first. We exercise the
//    blank-id tracker that replaced the `prior.length === 1` heuristic.
// 3. Clarifier resume cwd mismatch: the workbench filter now accepts a set
//    of cwds (worktree + project root) so a session whose cwd matches the
//    project root still surfaces in the workbench tab bar.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { selectWorkbenchTerminalSessions } from '../terminal/runtime/terminalSession.ts';
import { resolveProjectPath } from '../service/projectHelpers.ts';
import type { ProjectConfig } from '../../../global/types';

interface FakeTerminalRow {
  id: string;
  project: string;
  cwd: string;
}

const worktree = '/Users/u/.mando/worktrees/wt-A';
const projectRoot = '/Users/u/Code/myproj';

describe('selectWorkbenchTerminalSessions', () => {
  it('keeps sessions matching project + worktree cwd', () => {
    const sessions: FakeTerminalRow[] = [
      { id: 'a', project: 'myproj', cwd: worktree },
      { id: 'b', project: 'myproj', cwd: worktree },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, 'myproj', [worktree]);
    assert.deepEqual(
      out.map((s) => s.id),
      ['a', 'b'],
    );
  });

  it('drops sessions from sibling projects', () => {
    const sessions: FakeTerminalRow[] = [
      { id: 'a', project: 'myproj', cwd: worktree },
      { id: 'x', project: 'other', cwd: worktree },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, 'myproj', [worktree]);
    assert.deepEqual(
      out.map((s) => s.id),
      ['a'],
    );
  });

  it('drops sessions whose cwd does not match any accepted cwd', () => {
    const sessions: FakeTerminalRow[] = [
      { id: 'a', project: 'myproj', cwd: worktree },
      { id: 'p', project: 'myproj', cwd: '/somewhere/else' },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, 'myproj', [worktree]);
    assert.deepEqual(
      out.map((s) => s.id),
      ['a'],
    );
  });

  it('keeps clarifier-resumed session whose cwd is the project root (Bug 3)', () => {
    // Resumed clarifier terminal lives at the project root because the
    // clarifier's cc_sessions row stored cwd = project root. Without the
    // wider filter the resumed terminal would never make it into the tab
    // bar, so the panel would render "Resuming session..." indefinitely.
    const sessions: FakeTerminalRow[] = [
      { id: 'worker', project: 'myproj', cwd: worktree },
      { id: 'clarifier-resume', project: 'myproj', cwd: projectRoot },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, 'myproj', [worktree, projectRoot]);
    assert.deepEqual(out.map((s) => s.id).sort(), ['clarifier-resume', 'worker']);
  });
});

describe('resolveProjectPath', () => {
  const projects: Record<string, ProjectConfig> = {
    [projectRoot]: {
      name: 'myproj',
      path: projectRoot,
      aliases: ['mp', 'mine'],
    },
    '/other/path': {
      name: 'other',
      path: '/other/path',
      aliases: [],
    },
  };

  it('resolves by project name (case-insensitive)', () => {
    assert.equal(resolveProjectPath(projects, 'MyProj'), projectRoot);
  });

  it('resolves by alias (case-insensitive)', () => {
    assert.equal(resolveProjectPath(projects, 'MP'), projectRoot);
  });

  it('falls through to direct key match', () => {
    assert.equal(resolveProjectPath(projects, projectRoot), projectRoot);
  });

  it('returns null for an unknown project', () => {
    assert.equal(resolveProjectPath(projects, 'unknown'), null);
  });

  it('returns null when projects map is missing or empty', () => {
    assert.equal(resolveProjectPath(undefined, 'myproj'), null);
    assert.equal(resolveProjectPath({}, 'myproj'), null);
  });

  it('returns null when the matched entry has no stored path', () => {
    const partial: Record<string, ProjectConfig> = {
      foo: { name: 'foo', aliases: [] },
    };
    assert.equal(resolveProjectPath(partial, 'foo'), null);
  });
});

describe('blank-id tracking semantics (Bug 2)', () => {
  // Re-implements the operations the orchestration performs against
  // `blankIdsRef` so we exercise the contract without booting React. Only
  // the empty-workbench auto-create branch (`autoCreateBlank`) registers
  // ids in this set. Explicit user `+ Claude` / `+ Codex` clicks go
  // through `handleNewTerminal` and stay out — those are intentional
  // tabs that must survive a subsequent Resume.
  it('only deletes a blank that the orchestration auto-created', () => {
    const blankIds = new Set<string>();
    const cached: FakeTerminalRow[] = [];
    const acceptedCwds = new Set([worktree]);
    const project = 'myproj';

    // Step 1 — empty-workbench auto-create spawns a blank.
    const blank = { id: 'blank-1', project, cwd: worktree };
    cached.push(blank);
    blankIds.add(blank.id);

    // Step 2 — first Resume succeeds. Blank gets evicted.
    const firstResume = { id: 'resume-1', project, cwd: worktree };
    cached.push(firstResume);
    const blankToDelete1 = pickBlankToDelete(blankIds, cached, project, acceptedCwds);
    assert.equal(blankToDelete1, blank.id);
    blankIds.delete(blank.id);
    cached.splice(
      cached.findIndex((s) => s.id === blank.id),
      1,
    );

    // Step 3 — second Resume must NOT evict the first resumed terminal.
    const secondResume = { id: 'resume-2', project, cwd: worktree };
    cached.push(secondResume);
    const blankToDelete2 = pickBlankToDelete(blankIds, cached, project, acceptedCwds);
    assert.equal(
      blankToDelete2,
      null,
      'second resume must not target the previously-resumed terminal',
    );
    assert.deepEqual(
      cached.map((s) => s.id).sort(),
      ['resume-1', 'resume-2'],
      'both resumed terminals must coexist',
    );
  });

  it('a user-clicked + Claude tab survives a subsequent Resume (P1: Codex review)', () => {
    // The bug shape from the PR review: if every newly-created tab id
    // landed in `blankIdsRef`, a user who explicitly clicked "+ Claude"
    // would silently lose that tab the next time they clicked Resume.
    // Only `autoCreateBlank` (empty-workbench branch) registers in
    // `blankIdsRef`; `handleNewTerminal` stays out.
    const blankIds = new Set<string>();
    const cached: FakeTerminalRow[] = [];
    const acceptedCwds = new Set([worktree]);
    const project = 'myproj';

    // Empty workbench → auto-create runs once, then user resumes A which
    // evicts the auto-blank.
    const autoBlank = { id: 'auto-blank', project, cwd: worktree };
    cached.push(autoBlank);
    blankIds.add(autoBlank.id);

    const resumeA = { id: 'resume-a', project, cwd: worktree };
    cached.push(resumeA);
    const evict1 = pickBlankToDelete(blankIds, cached, project, acceptedCwds);
    assert.equal(evict1, autoBlank.id);
    blankIds.delete(autoBlank.id);
    cached.splice(
      cached.findIndex((s) => s.id === autoBlank.id),
      1,
    );

    // User clicks "+ Claude" — handleNewTerminal does NOT add to blankIds.
    const userBlank = { id: 'user-claude-tab', project, cwd: worktree };
    cached.push(userBlank);

    // User clicks Resume on session B. Eviction loop must find no blank
    // and leave both `resume-a` and `user-claude-tab` in place.
    const resumeB = { id: 'resume-b', project, cwd: worktree };
    cached.push(resumeB);
    const evict2 = pickBlankToDelete(blankIds, cached, project, acceptedCwds);
    assert.equal(evict2, null, 'resume must not target a user-opened tab');
    assert.deepEqual(
      cached.map((s) => s.id).sort(),
      ['resume-a', 'resume-b', 'user-claude-tab'],
      'user-clicked + Claude tab must survive subsequent resume',
    );
  });
});

function pickBlankToDelete(
  blankIds: Set<string>,
  cached: readonly FakeTerminalRow[],
  project: string,
  acceptedCwds: ReadonlySet<string>,
): string | null {
  for (const id of blankIds) {
    const match = cached.find(
      (s) => s.id === id && s.project === project && acceptedCwds.has(s.cwd),
    );
    if (match) return id;
  }
  return null;
}

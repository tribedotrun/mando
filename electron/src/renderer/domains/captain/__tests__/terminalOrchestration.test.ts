// Locks the terminal-resume bugs fixed in PR #983 plus the cross-pollination
// fix landed in this PR (#TODO-104):
//
// 1. Re-resume the same session id: relies on the orchestration's reactive
//    effect (not exercised here — pure JSX, requires a runner). Instead we
//    pin the underlying invariants below.
// 2. Resuming a second session must not evict the first. We exercise the
//    blank-id tracker that replaced the `prior.length === 1` heuristic.
// 3. Cross-workbench leak: the workbench filter now scopes by stamped
//    `workbenchId` instead of `project + cwd`, so a terminal whose cwd
//    happened to equal the project root no longer leaks into every
//    workbench in the project.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { selectWorkbenchTerminalSessions } from '../terminal/runtime/terminalSession.ts';
import { buildResumeTerminalCreateParams } from '../terminal/service/resumeTerminalCreate.ts';

interface FakeTerminalRow {
  id: string;
  workbenchId: number;
}

const WORKBENCH_A = 1;
const WORKBENCH_T = 2;

describe('selectWorkbenchTerminalSessions', () => {
  it('keeps sessions stamped with the requested workbench id', () => {
    const sessions: FakeTerminalRow[] = [
      { id: 'a', workbenchId: WORKBENCH_A },
      { id: 'b', workbenchId: WORKBENCH_A },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, WORKBENCH_A);
    assert.deepEqual(
      out.map((s) => s.id),
      ['a', 'b'],
    );
  });

  it('drops sessions stamped with a sibling workbench (cross-pollination fix)', () => {
    // The bug: a terminal whose cwd matched the project root used to
    // surface in every workbench of that project. Stamping workbench_id
    // at create time and filtering by identity here closes the leak.
    const sessions: FakeTerminalRow[] = [
      { id: 'a-shell', workbenchId: WORKBENCH_A },
      { id: 'projroot-shell', workbenchId: 3 },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, WORKBENCH_T);
    assert.deepEqual(
      out.map((s) => s.id),
      [],
      'workbench T must not see sessions owned by other workbenches',
    );
  });

  it('keeps clarifier-resumed session that the renderer pinned to this workbench', () => {
    // Resumed clarifier terminals inherit the cc_sessions row's cwd
    // (project root). The fix puts identity on the wire instead of
    // widening the cwd filter, so the resumed session still surfaces
    // here as long as its workbench_id matches.
    const sessions: FakeTerminalRow[] = [
      { id: 'worker', workbenchId: WORKBENCH_A },
      { id: 'clarifier-resume', workbenchId: WORKBENCH_A },
    ];
    const out = selectWorkbenchTerminalSessions(sessions, WORKBENCH_A);
    assert.deepEqual(out.map((s) => s.id).sort(), ['clarifier-resume', 'worker']);
  });
});

describe('buildResumeTerminalCreateParams', () => {
  it('preserves Codex as the provider-native resume agent', () => {
    assert.deepEqual(
      buildResumeTerminalCreateParams({
        workbenchId: WORKBENCH_A,
        project: 'mando',
        cwd: '/tmp/mando',
        sessionId: 'codex-thread-123',
        displayName: 'Worker #1',
        agent: 'codex',
      }),
      {
        workbenchId: WORKBENCH_A,
        project: 'mando',
        cwd: '/tmp/mando',
        agent: 'codex',
        resume_session_id: 'codex-thread-123',
        name: 'Worker #1',
      },
    );
  });

  it('defaults old resume URLs to Claude', () => {
    assert.equal(
      buildResumeTerminalCreateParams({
        workbenchId: WORKBENCH_A,
        project: 'mando',
        cwd: '/tmp/mando',
        sessionId: 'claude-session-123',
      }).agent,
      'claude',
    );
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

    // Step 1 — empty-workbench auto-create spawns a blank.
    const blank = { id: 'blank-1', workbenchId: WORKBENCH_A };
    cached.push(blank);
    blankIds.add(blank.id);

    // Step 2 — first Resume succeeds. Blank gets evicted.
    const firstResume = { id: 'resume-1', workbenchId: WORKBENCH_A };
    cached.push(firstResume);
    const blankToDelete1 = pickBlankToDelete(blankIds, cached, WORKBENCH_A);
    assert.equal(blankToDelete1, blank.id);
    blankIds.delete(blank.id);
    cached.splice(
      cached.findIndex((s) => s.id === blank.id),
      1,
    );

    // Step 3 — second Resume must NOT evict the first resumed terminal.
    const secondResume = { id: 'resume-2', workbenchId: WORKBENCH_A };
    cached.push(secondResume);
    const blankToDelete2 = pickBlankToDelete(blankIds, cached, WORKBENCH_A);
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

    // Empty workbench → auto-create runs once, then user resumes A which
    // evicts the auto-blank.
    const autoBlank = { id: 'auto-blank', workbenchId: WORKBENCH_A };
    cached.push(autoBlank);
    blankIds.add(autoBlank.id);

    const resumeA = { id: 'resume-a', workbenchId: WORKBENCH_A };
    cached.push(resumeA);
    const evict1 = pickBlankToDelete(blankIds, cached, WORKBENCH_A);
    assert.equal(evict1, autoBlank.id);
    blankIds.delete(autoBlank.id);
    cached.splice(
      cached.findIndex((s) => s.id === autoBlank.id),
      1,
    );

    // User clicks "+ Claude" — handleNewTerminal does NOT add to blankIds.
    const userBlank = { id: 'user-claude-tab', workbenchId: WORKBENCH_A };
    cached.push(userBlank);

    // User clicks Resume on session B. Eviction loop must find no blank
    // and leave both `resume-a` and `user-claude-tab` in place.
    const resumeB = { id: 'resume-b', workbenchId: WORKBENCH_A };
    cached.push(resumeB);
    const evict2 = pickBlankToDelete(blankIds, cached, WORKBENCH_A);
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
  workbenchId: number,
): string | null {
  for (const id of blankIds) {
    const match = cached.find((s) => s.id === id && s.workbenchId === workbenchId);
    if (match) return id;
  }
  return null;
}

type TerminalAgent = 'claude' | 'codex';

export function buildResumeTerminalCreateParams(input: {
  workbenchId: number;
  project: string;
  cwd: string;
  sessionId: string;
  displayName?: string | null;
  agent?: TerminalAgent | null;
}) {
  return {
    workbenchId: input.workbenchId,
    project: input.project,
    cwd: input.cwd,
    agent: input.agent ?? 'claude',
    resume_session_id: input.sessionId,
    name: input.displayName ?? undefined,
  };
}

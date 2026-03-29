import React from 'react';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

export type TaskMode = 'quick' | 'planned' | 'adopt';

export const MODE_COPY: Record<TaskMode, { title: string; summary: string; submit: string }> = {
  quick: {
    title: 'Quick task',
    summary: 'Captain clarifies from scratch, then spawns the normal worker flow.',
    submit: 'Create',
  },
  planned: {
    title: 'Planned task',
    summary: 'Human already planned the work. Mando skips clarifier and starts worker_briefed.',
    submit: 'Create',
  },
  adopt: {
    title: 'Adopt worktree',
    summary: 'Reuse an existing branch and worktree. Captain resumes with worker_continue.',
    submit: 'Adopt',
  },
};

const inputCls =
  'w-full rounded-md px-3 py-2 text-sm placeholder-[var(--color-text-3)] focus:outline-none focus:ring-1';
const inputStyle: React.CSSProperties = {
  border: '1px solid var(--color-border)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
};
const labelCls = 'mb-1 block text-xs font-medium uppercase tracking-wider';
const labelStyle: React.CSSProperties = { color: 'var(--color-text-3)' };
const modeCardBase =
  'rounded-xl border px-3 py-3 text-left transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]';

function FieldLabel({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <label className={labelCls} style={labelStyle}>
      {children}
    </label>
  );
}

export function ModeCard({
  mode,
  selected,
  onSelect,
}: {
  mode: TaskMode;
  selected: boolean;
  onSelect: (mode: TaskMode) => void;
}): React.ReactElement {
  const copy = MODE_COPY[mode];
  return (
    <button
      type="button"
      data-testid={`task-mode-${mode}`}
      onClick={() => onSelect(mode)}
      className={modeCardBase}
      style={{
        background: selected ? 'var(--color-accent-wash)' : 'var(--color-surface-2)',
        borderColor: selected ? 'var(--color-accent)' : 'var(--color-border-subtle)',
      }}
    >
      <div className="text-[13px] font-semibold" style={{ color: 'var(--color-text-1)' }}>
        {copy.title}
      </div>
      <div className="mt-1 text-[12px] leading-[17px]" style={{ color: 'var(--color-text-3)' }}>
        {copy.summary}
      </div>
    </button>
  );
}

interface PlannedTaskFieldsProps {
  linearId: string;
  onLinearIdChange: (value: string) => void;
  planPath: string;
  onPlanPathChange: (value: string) => void;
  context: string;
  onContextChange: (value: string) => void;
  noPr: boolean;
  onNoPrChange: (value: boolean) => void;
}

export function PlannedTaskFields({
  linearId,
  onLinearIdChange,
  planPath,
  onPlanPathChange,
  context,
  onContextChange,
  noPr,
  onNoPrChange,
}: PlannedTaskFieldsProps): React.ReactElement {
  return (
    <>
      <div className="grid gap-4 md:grid-cols-2">
        <div>
          <FieldLabel>Linear issue (optional)</FieldLabel>
          <input
            data-testid="task-linear-id-input"
            className={inputCls}
            style={inputStyle}
            value={linearId}
            onChange={(e) => onLinearIdChange(e.target.value)}
            placeholder="ABR-123"
          />
        </div>
        <div>
          <FieldLabel>Brief path</FieldLabel>
          <input
            data-testid="task-plan-input"
            className={inputCls}
            style={inputStyle}
            value={planPath}
            onChange={(e) => onPlanPathChange(e.target.value)}
            placeholder="~/.mando/plans/ABR-123/brief.md"
          />
        </div>
      </div>

      <div>
        <FieldLabel>Planning notes (optional)</FieldLabel>
        <textarea
          data-testid="task-context-input"
          className={`${inputCls} resize-none`}
          style={inputStyle}
          rows={4}
          value={context}
          onChange={(e) => onContextChange(e.target.value)}
          placeholder="Acceptance criteria, decisions from planning, or constraints Captain should preserve."
        />
      </div>

      <div
        className="flex items-center justify-between rounded-xl border px-3 py-3"
        style={{
          borderColor: 'var(--color-border-subtle)',
          background: 'var(--color-surface-2)',
        }}
      >
        <div>
          <div className="text-[13px] font-medium" style={{ color: 'var(--color-text-1)' }}>
            No PR needed
          </div>
          <div className="mt-1 text-[12px] leading-[17px]" style={{ color: 'var(--color-text-3)' }}>
            Use this for audits, investigations, or read-only findings that should stop at a workpad
            update.
          </div>
        </div>
        <ToggleSwitch
          testId="task-no-pr-toggle"
          checked={noPr}
          onChange={() => onNoPrChange(!noPr)}
        />
      </div>
    </>
  );
}

interface AdoptTaskFieldsProps {
  worktreePath: string;
  onWorktreePathChange: (value: string) => void;
  branch: string;
  onBranchChange: (value: string) => void;
  note: string;
  onNoteChange: (value: string) => void;
}

export function AdoptTaskFields({
  worktreePath,
  onWorktreePathChange,
  branch,
  onBranchChange,
  note,
  onNoteChange,
}: AdoptTaskFieldsProps): React.ReactElement {
  return (
    <>
      <div>
        <FieldLabel>Existing worktree path</FieldLabel>
        <input
          data-testid="task-worktree-input"
          className={inputCls}
          style={inputStyle}
          value={worktreePath}
          onChange={(e) => onWorktreePathChange(e.target.value)}
          placeholder="/absolute/path/to/worktree"
        />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <div>
          <FieldLabel>Branch (optional)</FieldLabel>
          <input
            data-testid="task-branch-input"
            className={inputCls}
            style={inputStyle}
            value={branch}
            onChange={(e) => onBranchChange(e.target.value)}
            placeholder="feature/my-branch"
          />
          <div className="mt-1 text-[12px]" style={{ color: 'var(--color-text-4)' }}>
            Leave blank to auto-detect from the checked-out branch.
          </div>
        </div>
        <div>
          <FieldLabel>Adopt note (optional)</FieldLabel>
          <input
            data-testid="task-note-input"
            className={inputCls}
            style={inputStyle}
            value={note}
            onChange={(e) => onNoteChange(e.target.value)}
            placeholder="What is done, what is risky, what remains"
          />
        </div>
      </div>
    </>
  );
}

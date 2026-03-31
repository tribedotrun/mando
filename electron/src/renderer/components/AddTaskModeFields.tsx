import React from 'react';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

export type TaskMode = 'quick' | 'planned';

export const MODE_COPY: Record<TaskMode, { title: string; submit: string }> = {
  quick: {
    title: 'Quick task',
    submit: 'Create',
  },
  planned: {
    title: 'Planned task',
    submit: 'Create',
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

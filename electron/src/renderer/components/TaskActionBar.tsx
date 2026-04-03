import React, { useState, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { reopenItem, reworkItem } from '#renderer/api';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useToastStore } from '#renderer/stores/toastStore';
import type { TaskItem } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { canReopen, canRework, canAsk, getErrorMessage } from '#renderer/utils';

type Action = 'reopen' | 'rework' | 'ask';

const STATUS_HINT: Record<string, { label: string; color: string }> = {
  'awaiting-review': { label: 'Ready for review', color: 'var(--color-success)' },
  escalated: { label: 'Escalated', color: 'var(--color-error)' },
  'needs-clarification': { label: 'Needs your input', color: 'var(--color-needs-human)' },
};

interface Props {
  item: TaskItem;
  /** When provided, Ask is routed to the Q&A section instead of inline. */
  onAsk?: (question: string) => void;
}

export function TaskActionBar({ item, onAsk }: Props): React.ReactElement | null {
  const [text, setText] = useState('');
  const [pendingAction, setPendingAction] = useState<Action | null>(null);
  const [completed, setCompleted] = useState<string | null>(null);
  const taskFetch = useTaskStore((s) => s.fetch);
  const queryClient = useQueryClient();
  const toast = useToastStore.getState;

  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const isClarification = item.status === 'needs-clarification';
  const showReopen = canReopen(item);
  const showRework = canRework(item);
  const showAsk = canAsk(item);
  const hint = STATUS_HINT[item.status];

  const handleAction = useCallback(
    async (action: Action) => {
      if (!text.trim()) return;
      setPendingAction(action);
      try {
        if (action === 'ask') {
          onAsk?.(text.trim());
          setText('');
          setPendingAction(null);
          return;
        }
        if (action === 'reopen') await reopenItem(item.id, text);
        else if (action === 'rework') await reworkItem(item.id, text);
        taskFetch();
        queryClient.invalidateQueries({ queryKey: ['task-detail-timeline', item.id] });
        queryClient.invalidateQueries({ queryKey: ['task-detail-pr', item.id] });
        const msg = action === 'reopen' ? 'Task reopened' : 'Rework requested';
        toast().add('success', msg);
        setCompleted(msg);
      } catch (err) {
        log.warn(`[TaskActionBar] ${action} failed for item ${item.id}:`, err);
        toast().add('error', getErrorMessage(err, `${action} failed`));
      } finally {
        setPendingAction(null);
      }
    },
    [text, item.id, taskFetch, queryClient, toast, onAsk],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && e.metaKey && text.trim()) {
        e.preventDefault();
        if (showReopen) handleAction('reopen');
        else handleAction('ask');
      }
    },
    [text, showReopen, handleAction],
  );

  // ── Early returns AFTER all hooks ──

  if (isFinalized) return null;

  // NeedsClarification is handled by ClarificationSection in the detail view.
  if (isClarification) return null;

  if (completed) {
    return (
      <div
        className="shrink-0 px-4 py-3 text-[12px] font-medium"
        style={{ color: 'var(--color-success)', borderTop: '1px solid var(--color-border-subtle)' }}
      >
        {completed}
      </div>
    );
  }

  const placeholder =
    showReopen || showRework ? 'Feedback or question...' : 'Ask about this task...';

  return (
    <div className="shrink-0 pr-4 pt-3 pb-2">
      {/* Status hint */}
      {hint && (
        <div className="mb-1.5 text-[10px] font-medium" style={{ color: hint.color }}>
          {hint.label}
        </div>
      )}

      {/* Input row */}
      <div
        className="flex items-center gap-2 rounded-lg px-3 py-2"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
        }}
      >
        <textarea
          className="min-h-[20px] flex-1 resize-none bg-transparent text-[13px] leading-snug focus:outline-none"
          style={{ color: 'var(--color-text-1)' }}
          rows={1}
          placeholder={placeholder}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={!!pendingAction}
        />
        <div className="flex shrink-0 items-center gap-1.5">
          {showRework && (
            <ActionBtn
              label="Rework"
              onClick={() => handleAction('rework')}
              disabled={!text.trim() || !!pendingAction}
              pending={pendingAction === 'rework'}
            />
          )}
          {showReopen && (
            <ActionBtn
              label="Reopen"
              onClick={() => handleAction('reopen')}
              disabled={!text.trim() || !!pendingAction}
              pending={pendingAction === 'reopen'}
              accent
            />
          )}
          {(showAsk || (!showReopen && !showRework)) && (
            <ActionBtn
              label="Ask"
              onClick={() => handleAction('ask')}
              disabled={!text.trim() || !!pendingAction}
              pending={pendingAction === 'ask'}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function ActionBtn({
  label,
  onClick,
  disabled,
  pending,
  accent,
}: {
  label: string;
  onClick: () => void;
  disabled: boolean;
  pending: boolean;
  accent?: boolean;
}): React.ReactElement {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="rounded-md px-3 py-1 text-[11px] font-medium disabled:opacity-40"
      style={{
        background: accent ? 'var(--color-accent)' : 'transparent',
        color: accent ? 'var(--color-bg)' : 'var(--color-text-2)',
        border: accent ? 'none' : '1px solid var(--color-border)',
        cursor: disabled ? 'default' : 'pointer',
        lineHeight: '18px',
      }}
    >
      {pending ? '...' : label}
    </button>
  );
}

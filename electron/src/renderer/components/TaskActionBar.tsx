import React, { useState, useCallback } from 'react';
import log from '#renderer/logger';
import { reopenItem, reworkItem, answerClarification, askTask } from '#renderer/api';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useToastStore } from '#renderer/stores/toastStore';
import type { TaskItem } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { canReopen, canRework, canAsk, getErrorMessage } from '#renderer/utils';

type Action = 'reopen' | 'rework' | 'answer' | 'ask';

const STATUS_HINT: Record<string, { label: string; color: string }> = {
  'awaiting-review': { label: 'Ready for review', color: 'var(--color-success)' },
  escalated: { label: 'Escalated', color: 'var(--color-error)' },
  'needs-clarification': { label: 'Needs your input', color: 'var(--color-needs-human)' },
};

interface Props {
  item: TaskItem;
}

export function TaskActionBar({ item }: Props): React.ReactElement | null {
  const [text, setText] = useState('');
  const [pendingAction, setPendingAction] = useState<Action | null>(null);
  const [completed, setCompleted] = useState<string | null>(null);
  const [askHistory, setAskHistory] = useState<{ role: string; text: string }[]>([]);
  const taskFetch = useTaskStore((s) => s.fetch);
  const toast = useToastStore.getState;

  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const isClarification = item.status === 'needs-clarification';
  const showReopen = canReopen(item);
  const showRework = canRework(item);
  const showAsk = canAsk(item);
  const showAnswer = isClarification;
  const hint = STATUS_HINT[item.status];

  const handleAction = useCallback(
    async (action: Action) => {
      if (!text.trim()) return;
      setPendingAction(action);
      try {
        if (action === 'ask') {
          setAskHistory((h) => [...h, { role: 'user', text }]);
          const data = await askTask(item.id, text);
          setAskHistory((h) => [...h, { role: 'assistant', text: data.answer }]);
          setText('');
          return;
        }
        if (action === 'reopen') await reopenItem(item.id, text);
        else if (action === 'rework') await reworkItem(item.id, text);
        else {
          const result = await answerClarification(item.id, text);
          taskFetch();
          const msgs: Record<string, [variant: 'success' | 'info', msg: string]> = {
            ready: ['success', 'Clarified — task queued'],
            clarifying: ['info', 'Still needs more info'],
            escalate: ['info', 'Escalated to captain review'],
          };
          const [variant, msg] = msgs[result.status] ?? ['success', 'Answer saved'];
          toast().add(variant, msg);
          if (result.status !== 'clarifying') setCompleted(msg);
          else setText('');
          return;
        }
        taskFetch();
        const msg = action === 'reopen' ? 'Task reopened' : 'Rework requested';
        toast().add('success', msg);
        setCompleted(msg);
      } catch (err) {
        log.warn(`[TaskActionBar] ${action} failed for item ${item.id}:`, err);
        toast().add('error', getErrorMessage(err, `${action} failed`));
        if (action === 'ask') {
          setAskHistory((h) => [
            ...h,
            { role: 'assistant', text: `Error: ${getErrorMessage(err, 'Failed')}` },
          ]);
        }
      } finally {
        setPendingAction(null);
      }
    },
    [text, item.id, taskFetch, toast],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && e.metaKey && text.trim()) {
        e.preventDefault();
        if (showAnswer) handleAction('answer');
        else if (showReopen) handleAction('reopen');
        else handleAction('ask');
      }
    },
    [text, showAnswer, showReopen, handleAction],
  );

  // ── Early returns AFTER all hooks ──

  if (isFinalized) return null;

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

  let placeholder = 'Ask about this task...';
  if (showAnswer) placeholder = 'Provide your answer...';
  else if (showReopen || showRework) placeholder = 'Feedback or question...';

  return (
    <div
      className="shrink-0 pr-4 pt-3 pb-2"
      style={{ borderTop: '1px solid var(--color-border-subtle)' }}
    >
      {/* Ask history */}
      {askHistory.length > 0 && (
        <div className="mb-2 max-h-[200px] overflow-auto">
          {askHistory.map((entry, i) => (
            <div
              key={i}
              className="mb-1.5 rounded px-3 py-2 text-[12px] leading-relaxed"
              style={{
                background:
                  entry.role === 'user' ? 'var(--color-accent-wash)' : 'var(--color-surface-2)',
                color: entry.role === 'user' ? 'var(--color-text-1)' : 'var(--color-text-2)',
                whiteSpace: 'pre-wrap',
              }}
            >
              {entry.text}
            </div>
          ))}
        </div>
      )}

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
          {showAnswer && (
            <ActionBtn
              label="Answer"
              onClick={() => handleAction('answer')}
              disabled={!text.trim() || !!pendingAction}
              pending={pendingAction === 'answer'}
              accent
            />
          )}
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
          {(showAsk || (!showAnswer && !showReopen && !showRework)) && (
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

import React, { useState, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { reopenItem, reworkItem } from '#renderer/api';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useToastStore } from '#renderer/stores/toastStore';
import type { TaskItem } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { canReopen, canRework, canAsk, getErrorMessage } from '#renderer/utils';

type Action = 'ask' | 'reopen' | 'rework';

const ACTION_CONFIG: Record<
  Action,
  { label: string; placeholder: string; requiresInput: boolean }
> = {
  ask: { label: 'Ask', placeholder: 'Ask about this task...', requiresInput: true },
  reopen: { label: 'Reopen', placeholder: 'Feedback for reopen...', requiresInput: true },
  rework: { label: 'Rework', placeholder: 'Feedback for rework...', requiresInput: true },
};

interface Props {
  item: TaskItem;
  onAsk?: (question: string) => void;
}

export function TaskActionBar({ item, onAsk }: Props): React.ReactElement | null {
  const available = getAvailableActions(item);
  const defaultAction = getDefaultAction(item);
  const [selectedAction, setSelectedAction] = useState<Action>(defaultAction);
  const [text, setText] = useState('');
  const [pendingAction, setPendingAction] = useState<Action | null>(null);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  // No useRef needed — onBlur handles outside-click dismiss.
  const taskFetch = useTaskStore((s) => s.fetch);
  const queryClient = useQueryClient();
  const toast = useToastStore.getState;

  // Sync selected action when task status changes — only when there are valid actions.
  if (available.length > 0 && !available.includes(selectedAction)) {
    setSelectedAction(defaultAction);
  }

  const handleSubmit = useCallback(async () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setPendingAction(selectedAction);
    try {
      if (selectedAction === 'ask') {
        onAsk?.(trimmed);
        setText('');
        setPendingAction(null);
        return;
      }
      if (selectedAction === 'reopen') await reopenItem(item.id, trimmed);
      else if (selectedAction === 'rework') await reworkItem(item.id, trimmed);
      taskFetch();
      queryClient.invalidateQueries({ queryKey: ['task-detail-timeline', item.id] });
      queryClient.invalidateQueries({ queryKey: ['task-detail-pr', item.id] });
      const msg = selectedAction === 'reopen' ? 'Task reopened' : 'Rework requested';
      toast().add('success', msg);
      setText('');
    } catch (err) {
      log.warn(`[TaskActionBar] ${selectedAction} failed for item ${item.id}:`, err);
      toast().add('error', getErrorMessage(err, `${selectedAction} failed`));
    } finally {
      setPendingAction(null);
    }
  }, [text, selectedAction, item.id, taskFetch, queryClient, toast, onAsk]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && e.metaKey && text.trim()) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [text, handleSubmit],
  );

  // Hide for finalized, clarification (handled in status card), and queued states.
  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  if (isFinalized) return null;
  if (item.status === 'needs-clarification') return null;
  if (item.status === 'new' || item.status === 'queued') return null;
  if (available.length === 0) return null;

  const config = ACTION_CONFIG[selectedAction];
  const hasMultipleActions = available.length > 1;
  const canSubmit = text.trim().length > 0 && !pendingAction;

  return (
    <div
      className="shrink-0 px-4 pt-3 pb-3"
      style={{ borderTop: '1px solid var(--color-border-subtle)' }}
    >
      <div
        className="flex items-center gap-2 rounded-lg px-3 py-2"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
        }}
      >
        <textarea
          className="min-h-[20px] flex-1 resize-none bg-transparent text-body leading-snug focus:outline-none"
          style={{ color: 'var(--color-text-1)' }}
          rows={1}
          placeholder={config.placeholder}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={!!pendingAction}
        />

        {/* Split button */}
        <div
          className="relative flex shrink-0 items-center"
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget)) setDropdownOpen(false);
          }}
        >
          <button
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="rounded-l-md px-3 py-1 text-label font-medium disabled:opacity-40"
            style={{
              background: 'var(--color-accent)',
              color: 'var(--color-bg)',
              border: 'none',
              borderRight: hasMultipleActions ? '1px solid var(--color-accent-pressed)' : 'none',
              borderRadius: hasMultipleActions ? undefined : '6px',
              cursor: canSubmit ? 'pointer' : 'default',
              lineHeight: '14px',
            }}
          >
            {pendingAction ? '...' : config.label}
          </button>

          {hasMultipleActions && (
            <button
              onClick={() => setDropdownOpen((v) => !v)}
              className="rounded-r-md px-1.5 py-1"
              style={{
                background: 'var(--color-accent)',
                color: 'var(--color-bg)',
                border: 'none',
                cursor: 'pointer',
                lineHeight: '14px',
              }}
            >
              <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
                <path d="M2.5 4l2.5 2.5L7.5 4" />
              </svg>
            </button>
          )}

          {dropdownOpen && (
            <div
              className="absolute bottom-full right-0 z-50 mb-1 min-w-[120px] rounded-lg py-1"
              style={{
                background: 'var(--color-surface-3)',
                border: '1px solid var(--color-border)',
                boxShadow: '0 4px 16px rgba(0,0,0,0.3)',
              }}
            >
              {available.map((action) => (
                <button
                  key={action}
                  onClick={() => {
                    setSelectedAction(action);
                    setDropdownOpen(false);
                  }}
                  className="flex w-full items-center justify-between px-3 py-1.5 text-left text-caption hover:bg-[var(--color-surface-2)]"
                  style={{
                    color: 'var(--color-text-1)',
                    background: 'none',
                    border: 'none',
                    cursor: 'pointer',
                  }}
                >
                  {ACTION_CONFIG[action].label}
                  {action === selectedAction && (
                    <span style={{ color: 'var(--color-accent)' }}>&#10003;</span>
                  )}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function getAvailableActions(item: TaskItem): Action[] {
  const actions: Action[] = [];
  if (
    canAsk(item) ||
    ['in-progress', 'captain-reviewing', 'captain-merging', 'clarifying'].includes(item.status)
  ) {
    actions.push('ask');
  }
  if (canReopen(item)) actions.push('reopen');
  if (canRework(item)) actions.push('rework');
  return actions;
}

function getDefaultAction(item: TaskItem): Action {
  const available = getAvailableActions(item);
  // Ask is preferred default; fall back to first available action.
  if (available.includes('ask')) return 'ask';
  return available[0] ?? 'ask';
}

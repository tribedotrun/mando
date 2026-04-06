import React, { useState, useCallback, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { reopenItem, reworkItem } from '#renderer/domains/captain/hooks/useApi';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useToastStore } from '#renderer/global/stores/toastStore';
import type { TaskItem } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { canReopen, canRework, canAskAny, getErrorMessage } from '#renderer/utils';
import { invalidateTaskDetail } from '#renderer/queryClient';

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
  const textareaRef = useRef<HTMLTextAreaElement>(null);
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
        if (textareaRef.current) textareaRef.current.style.height = 'auto';
        setPendingAction(null);
        return;
      }
      if (selectedAction === 'reopen') await reopenItem(item.id, trimmed);
      else if (selectedAction === 'rework') await reworkItem(item.id, trimmed);
      taskFetch();
      invalidateTaskDetail(queryClient, item.id);
      const msg = selectedAction === 'reopen' ? 'Task reopened' : 'Rework requested';
      toast().add('success', msg);
      setText('');
      if (textareaRef.current) textareaRef.current.style.height = 'auto';
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

  // Determine visibility — hidden states still reserve no space via height collapse.
  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const hidden =
    isFinalized ||
    item.status === 'needs-clarification' ||
    item.status === 'captain-reviewing' ||
    item.status === 'new' ||
    item.status === 'queued' ||
    available.length === 0;

  const config = ACTION_CONFIG[selectedAction];
  const hasMultipleActions = available.length > 1;
  const canSubmit = !hidden && text.trim().length > 0 && !pendingAction;

  return (
    <div
      className="shrink-0"
      style={{
        maxHeight: hidden ? 0 : '200px',
        opacity: hidden ? 0 : 1,
        overflow: hidden ? 'hidden' : 'visible',
        transition: 'max-height 150ms ease, opacity 150ms ease',
        borderTop: hidden ? 'none' : '1px solid var(--color-border-subtle)',
      }}
    >
      <div className="px-4 pt-3 pb-3">
        <div
          className="flex items-end gap-2 rounded-lg px-3 py-2"
          style={{
            background: 'var(--color-surface-2)',
            border: '1px solid var(--color-border-subtle)',
          }}
        >
          <textarea
            ref={textareaRef}
            className="min-h-[20px] max-h-[120px] flex-1 resize-none overflow-y-auto bg-transparent text-body leading-snug focus:outline-none"
            style={{ color: 'var(--color-text-1)' }}
            rows={1}
            placeholder={config.placeholder}
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              e.target.style.height = 'auto';
              e.target.style.height = e.target.scrollHeight + 'px';
            }}
            onKeyDown={handleKeyDown}
            disabled={!!pendingAction}
          />

          {/* Action selector — text dropdown */}
          {hasMultipleActions && (
            <div
              className="relative shrink-0"
              onBlur={(e) => {
                if (!e.currentTarget.contains(e.relatedTarget)) setDropdownOpen(false);
              }}
            >
              <button
                onClick={() => setDropdownOpen((v) => !v)}
                className="flex items-center gap-1"
                style={{
                  background: 'none',
                  border: 'none',
                  color: 'var(--color-text-2)',
                  fontSize: '13px',
                  fontWeight: 500,
                  cursor: 'pointer',
                  padding: '4px 8px',
                  lineHeight: '18px',
                }}
              >
                {config.label}
                <svg
                  width="10"
                  height="10"
                  viewBox="0 0 10 10"
                  fill="currentColor"
                  style={{ opacity: 0.6 }}
                >
                  <path d="M3 4l2 2 2-2" />
                </svg>
              </button>

              {dropdownOpen && (
                <div
                  className="absolute bottom-full left-0 z-50 mb-1 min-w-[120px] rounded-lg py-1"
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
          )}

          {/* Circular send button */}
          <button
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="flex shrink-0 items-center justify-center"
            style={{
              width: '28px',
              height: '28px',
              borderRadius: '50%',
              background: canSubmit ? 'var(--color-accent)' : 'var(--color-surface-3)',
              color: canSubmit ? 'var(--color-bg)' : 'var(--color-text-3)',
              border: 'none',
              cursor: canSubmit ? 'pointer' : 'default',
              transition: 'background 150ms ease, color 150ms ease',
            }}
          >
            {pendingAction ? (
              <span style={{ fontSize: '12px' }}>&hellip;</span>
            ) : (
              <svg
                width="14"
                height="14"
                viewBox="0 0 14 14"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M7 12V3" />
                <path d="M3 6l4-4 4 4" />
              </svg>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

function getAvailableActions(item: TaskItem): Action[] {
  const actions: Action[] = [];
  if (canAskAny(item)) actions.push('ask');
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

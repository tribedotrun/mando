import React, { useState, useCallback, useRef } from 'react';
import { ArrowUp, ChevronDown } from 'lucide-react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { reopenItem, reworkItem } from '#renderer/domains/captain/hooks/useApi';
import { useDraft } from '#renderer/global/hooks/useDraft';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { toast } from 'sonner';
import type { TaskItem } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { canReopen, canRework, canAskAny, getErrorMessage } from '#renderer/utils';
import { invalidateTaskDetail } from '#renderer/queryClient';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
} from '#renderer/global/components/DropdownMenu';

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
  const [text, setText, clearTextDraft] = useDraft(
    `mando:draft:action:${item.id}:${selectedAction}`,
  );
  const [pendingAction, setPendingAction] = useState<Action | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const taskFetch = useTaskStore((s) => s.fetch);
  const queryClient = useQueryClient();
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
        clearTextDraft();
        if (textareaRef.current) textareaRef.current.style.height = 'auto';
        setPendingAction(null);
        return;
      }
      if (selectedAction === 'reopen') await reopenItem(item.id, trimmed);
      else if (selectedAction === 'rework') await reworkItem(item.id, trimmed);
      taskFetch();
      invalidateTaskDetail(queryClient, item.id);
      const msg = selectedAction === 'reopen' ? 'Task reopened' : 'Rework requested';
      toast.success(msg);
      clearTextDraft();
      if (textareaRef.current) textareaRef.current.style.height = 'auto';
    } catch (err) {
      log.warn(`[TaskActionBar] ${selectedAction} failed for item ${item.id}:`, err);
      toast.error(getErrorMessage(err, `${selectedAction} failed`));
    } finally {
      setPendingAction(null);
    }
  }, [text, selectedAction, item.id, taskFetch, queryClient, onAsk, clearTextDraft]);

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
            className="min-h-[20px] max-h-[120px] flex-1 resize-none overflow-y-auto bg-transparent py-1 text-body leading-snug text-text-1 focus:outline-none"
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

          {hasMultipleActions && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  disabled={!!pendingAction}
                  className="flex shrink-0 items-center gap-1"
                  style={{
                    background: 'none',
                    border: 'none',
                    color: 'var(--color-text-2)',
                    fontSize: '13px',
                    fontWeight: 500,
                    cursor: pendingAction ? 'default' : 'pointer',
                    padding: '4px 8px',
                    lineHeight: '18px',
                  }}
                >
                  {config.label}
                  <ChevronDown size={10} style={{ opacity: 0.6 }} />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent side="top" align="start" className="min-w-[120px]">
                {available.map((action) => (
                  <DropdownMenuCheckboxItem
                    key={action}
                    checked={action === selectedAction}
                    onSelect={() => setSelectedAction(action)}
                  >
                    {ACTION_CONFIG[action].label}
                  </DropdownMenuCheckboxItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
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
              <ArrowUp size={14} strokeWidth={2} />
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

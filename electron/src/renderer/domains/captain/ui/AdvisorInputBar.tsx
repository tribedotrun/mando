import React, { useCallback, useRef, useState } from 'react';
import { ArrowUp, Loader2 } from 'lucide-react';
import type { TaskItem } from '#renderer/global/types';
import { cn } from '#renderer/global/service/cn';
import { canReopen, canRework, canRevisePlan, clamp } from '#renderer/global/service/utils';

export function AdvisorInputBar({
  item,
  onSend,
  isPending,
}: {
  item: TaskItem;
  onSend: (message: string, intent: string) => void;
  isPending: boolean;
}): React.ReactElement {
  const [input, setInput] = useState('');
  const [intent, setIntent] = useState<'ask' | 'reopen' | 'rework' | 'revise-plan'>('ask');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed || isPending) return;
    onSend(trimmed, intent);
    setInput('');
    setIntent('ask');
  }, [input, isPending, onSend, intent]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleInput = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = `${clamp(el.scrollHeight, 56, 256)}px`;
  }, []);

  const showReopen = canReopen(item);
  const showRework = canRework(item);
  const showRevise = canRevisePlan(item);

  return (
    <div className="bg-background px-2 pb-1.5">
      <div
        className={cn(
          'rounded-xl border bg-surface-1 transition-colors',
          intent !== 'ask' ? 'border-accent/40' : 'border-border',
          'focus-within:border-text-3',
        )}
      >
        <textarea
          ref={textareaRef}
          value={input}
          onChange={handleInput}
          onKeyDown={handleKeyDown}
          placeholder={
            intent === 'reopen'
              ? 'Describe what to fix (sends as reopen)...'
              : intent === 'rework'
                ? 'Describe what to redo (fresh worker + new branch)...'
                : intent === 'revise-plan'
                  ? 'Describe what to change in the plan (re-runs planning)...'
                  : 'Ask the advisor about this task...'
          }
          rows={2}
          className="min-h-[52px] max-h-[256px] w-full resize-none border-0 bg-transparent px-3.5 pt-3 pb-0 text-body leading-5 text-text-1 placeholder:text-text-3 focus:outline-none"
        />
        <div className="flex items-center justify-between px-1.5 pb-1.5">
          <div>
            {showReopen || showRework || showRevise ? (
              <select
                value={intent}
                onChange={(e) =>
                  setIntent(e.target.value as 'ask' | 'reopen' | 'rework' | 'revise-plan')
                }
                className="cursor-pointer appearance-none rounded-md bg-transparent py-1 pr-4 pl-2 text-body text-text-3 hover:text-text-1 focus:outline-none"
                style={{
                  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 24 24' fill='none' stroke='%23666' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E")`,
                  backgroundRepeat: 'no-repeat',
                  backgroundPosition: 'right 3px center',
                }}
              >
                <option value="ask">Ask</option>
                {showReopen && <option value="reopen">Reopen</option>}
                {showRework && <option value="rework">Rework</option>}
                {showRevise && <option value="revise-plan">Revise</option>}
              </select>
            ) : (
              <span className="py-1 pl-2 text-body text-text-4">Ask</span>
            )}
          </div>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!input.trim() || isPending}
            className={cn(
              'flex h-7 w-7 items-center justify-center rounded-lg transition-all duration-150',
              input.trim() && !isPending
                ? 'bg-text-1 text-background hover:opacity-80'
                : 'text-text-4',
            )}
          >
            {isPending ? <Loader2 size={14} className="animate-spin" /> : <ArrowUp size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}

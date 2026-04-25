import React from 'react';
import { ArrowUp, Loader2 } from 'lucide-react';
import type { TaskItem } from '#renderer/global/types';
import { cn } from '#renderer/global/service/cn';
import {
  useAdvisorInputBar,
  type AdvisorIntent,
} from '#renderer/domains/captain/runtime/useAdvisorInputBar';

export function AdvisorInputBar({
  item,
  onSend,
  isPending,
}: {
  item: TaskItem;
  onSend: (message: string, intent: AdvisorIntent) => void;
  isPending: boolean;
}): React.ReactElement {
  const bar = useAdvisorInputBar({ item, onSend, isPending });
  const { text, events, intent } = bar;
  const showSelect = intent.canReopen || intent.canRework || intent.canRevise;

  return (
    <div className="bg-background px-2 pb-1.5">
      <div
        className={cn(
          'rounded-xl border bg-surface-1 transition-colors',
          intent.value !== 'ask' ? 'border-accent/40' : 'border-border',
          'focus-within:border-text-3',
        )}
      >
        <textarea
          ref={text.textareaRef}
          value={text.input}
          onChange={text.handleInput}
          onKeyDown={events.handleKeyDown}
          placeholder={
            intent.value === 'reopen'
              ? 'Describe what to fix (sends as reopen)...'
              : intent.value === 'rework'
                ? 'Describe what to redo (fresh worker + new branch)...'
                : intent.value === 'revise-plan'
                  ? 'Describe what to change in the plan (re-runs planning)...'
                  : 'Ask the advisor about this task...'
          }
          rows={2}
          className="min-h-[52px] max-h-[256px] w-full resize-none border-0 bg-transparent px-3.5 pt-3 pb-0 text-body leading-5 text-text-1 placeholder:text-text-3 focus:outline-none"
        />
        <div className="flex items-center justify-between px-1.5 pb-1.5">
          <div>
            {showSelect ? (
              <select
                value={intent.value}
                onChange={(e) => intent.set(e.target.value as AdvisorIntent)}
                className="cursor-pointer appearance-none rounded-md bg-transparent py-1 pr-4 pl-2 text-body text-text-3 hover:text-text-1 focus:outline-none"
                style={{
                  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 24 24' fill='none' stroke='%23666' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E")`,
                  backgroundRepeat: 'no-repeat',
                  backgroundPosition: 'right 3px center',
                }}
              >
                <option value="ask">Ask</option>
                {intent.canReopen && <option value="reopen">Reopen</option>}
                {intent.canRework && <option value="rework">Rework</option>}
                {intent.canRevise && <option value="revise-plan">Revise</option>}
              </select>
            ) : (
              <span className="py-1 pl-2 text-body text-text-4">Ask</span>
            )}
          </div>
          <button
            type="button"
            onClick={events.handleSubmit}
            disabled={!bar.canSubmit}
            className={cn(
              'flex h-7 w-7 items-center justify-center rounded-lg transition-all duration-150',
              bar.canSubmit ? 'bg-text-1 text-background hover:opacity-80' : 'text-text-4',
            )}
          >
            {isPending ? <Loader2 size={14} className="animate-spin" /> : <ArrowUp size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}

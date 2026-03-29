import React from 'react';
import { getBaseUrl } from '#renderer/api';
import type { TaskItem } from '#renderer/types';
import { CopyBtn } from '#renderer/components/CopyBtn';

export function ExpandedDetails({ item }: { item: TaskItem }): React.ReactElement | null {
  const hasContent =
    item.original_prompt ||
    item.context ||
    item.images ||
    item.worktree ||
    item.plan ||
    item.branch ||
    item.linear_id ||
    item.no_pr ||
    item.escalation_report ||
    item.clarifier_questions ||
    item.captain_review_trigger ||
    item.intervention_count > 0;
  if (!hasContent) return null;

  return (
    <div className="px-6 py-3" style={{ background: 'var(--color-surface-2)' }}>
      <div className="space-y-1 text-[12px]" style={{ color: 'var(--color-text-2)' }}>
        {item.original_prompt && (
          <div>
            <span style={{ color: 'var(--color-text-3)' }}>request:</span>{' '}
            <em>{item.original_prompt}</em>
          </div>
        )}
        {item.context && !item.original_prompt && (
          <div>
            <span style={{ color: 'var(--color-text-3)' }}>context:</span> {item.context}
          </div>
        )}
        {item.linear_id && (
          <div className="flex items-center gap-2">
            <span style={{ color: 'var(--color-text-3)' }}>linear:</span>
            <code className="text-code" style={{ color: 'var(--color-text-1)' }}>
              {item.linear_id}
            </code>
            <CopyBtn text={item.linear_id} label="copy" />
          </div>
        )}
        {item.plan && (
          <div className="flex items-center gap-2">
            <span style={{ color: 'var(--color-text-3)' }}>
              {item.plan.endsWith('adopt-handoff.md') ? 'adopt:' : 'brief:'}
            </span>
            <code className="text-code" style={{ color: 'var(--color-text-1)' }}>
              {item.plan}
            </code>
            <CopyBtn text={item.plan} label="copy" />
          </div>
        )}
        {item.branch && (
          <div className="flex items-center gap-2">
            <span style={{ color: 'var(--color-text-3)' }}>branch:</span>
            <code className="text-code" style={{ color: 'var(--color-text-1)' }}>
              {item.branch}
            </code>
            <CopyBtn text={item.branch} label="copy" />
          </div>
        )}
        {item.no_pr && (
          <div>
            <span style={{ color: 'var(--color-text-3)' }}>delivery:</span>{' '}
            <span style={{ color: 'var(--color-accent)', fontWeight: 500 }}>
              findings only — no PR required
            </span>
          </div>
        )}
        {item.intervention_count > 0 && <InterventionBar count={item.intervention_count} />}
        {item.status === 'escalated' && item.escalation_report && (
          <div>
            <span style={{ color: 'var(--color-error)', fontWeight: 500 }}>escalation report:</span>
            <pre
              className="mt-1 whitespace-pre-wrap rounded p-2 text-[11px]"
              style={{
                background: 'var(--color-surface-3)',
                color: 'var(--color-text-1)',
                border: '1px solid var(--color-border-subtle)',
              }}
            >
              {item.escalation_report}
            </pre>
          </div>
        )}
        {item.status === 'needs-clarification' && item.clarifier_questions && (
          <div>
            <span style={{ color: 'var(--color-needs-human)', fontWeight: 500 }}>
              questions from captain:
            </span>
            <pre
              className="mt-1 whitespace-pre-wrap rounded p-2 text-[11px]"
              style={{
                background: 'var(--color-surface-3)',
                color: 'var(--color-text-1)',
                border: '1px solid var(--color-border-subtle)',
              }}
            >
              {item.clarifier_questions}
            </pre>
          </div>
        )}
        {item.status === 'captain-reviewing' && item.captain_review_trigger && (
          <div>
            <span style={{ color: 'var(--color-accent)', fontWeight: 500 }}>review trigger:</span>{' '}
            {item.captain_review_trigger}
          </div>
        )}
        {item.images && (
          <div>
            <span style={{ color: 'var(--color-text-3)' }}>images:</span>
            <div className="mt-1 flex flex-wrap gap-2">
              {item.images.split(',').map((img) => {
                const f = img.trim();
                if (!f) return null;
                const src = `${getBaseUrl()}/api/images/${f}`;
                return (
                  <a key={f} href={src} target="_blank" rel="noopener noreferrer">
                    <img
                      src={src}
                      alt={f}
                      className="h-12 w-12 rounded object-cover"
                      style={{ border: '1px solid var(--color-border)' }}
                    />
                  </a>
                );
              })}
            </div>
          </div>
        )}
        {item.worktree && (
          <div className="flex items-center gap-2">
            <span style={{ color: 'var(--color-text-3)' }}>dir:</span>
            <code className="text-code" style={{ color: 'var(--color-text-1)' }}>
              {item.worktree}
            </code>
            <CopyBtn text={item.worktree} label="copy" />
          </div>
        )}
      </div>
    </div>
  );
}

function InterventionBar({ count, max = 50 }: { count: number; max?: number }): React.ReactElement {
  const ratio = count / max;
  const pct = Math.min(ratio * 100, 100);
  const barColor =
    ratio > 0.8
      ? 'var(--color-error)'
      : ratio > 0.5
        ? 'var(--color-stale)'
        : 'var(--color-success)';
  return (
    <div className="flex items-center gap-2">
      <span style={{ color: 'var(--color-text-3)' }}>interventions:</span>
      <div className="h-1.5 w-20 rounded-full" style={{ background: 'var(--color-surface-3)' }}>
        <div
          className="h-full rounded-full"
          style={{ width: `${pct}%`, backgroundColor: barColor }}
        />
      </div>
      <span className="font-mono text-[11px]" style={{ color: 'var(--color-text-3)' }}>
        {count}/{max}
      </span>
    </div>
  );
}

export function TaskEmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center py-16">
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none" className="mb-4">
        <rect
          x="8"
          y="8"
          width="32"
          height="32"
          rx="6"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
        />
        <path
          d="M18 24l4 4 8-8"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      <span className="text-subheading mb-1" style={{ color: 'var(--color-text-2)' }}>
        No tasks yet
      </span>
      <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
        Create a task and Captain will pick it up automatically.
      </span>
    </div>
  );
}

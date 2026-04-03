import React from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import type { TaskItem } from '#renderer/types';

/* ── Status indicator ── */

/** Human-action states get a subtle inline label before the title */
export const ACTION_LABELS: Record<string, { color: string; label: string }> = {
  'awaiting-review': { color: 'var(--color-success)', label: 'Review' },
  escalated: { color: 'var(--color-error)', label: 'Escalated' },
  'needs-clarification': { color: 'var(--color-needs-human)', label: 'Needs input' },
};

/** Human-readable tooltip for each status */
export const STATUS_TOOLTIP: Record<string, string> = {
  new: 'Queued',
  queued: 'Queued',
  clarifying: 'Clarifying',
  'in-progress': 'Working',
  'captain-reviewing': 'Reviewing',
  'captain-merging': 'Merging',
  'awaiting-review': 'Awaiting review',
  escalated: 'Escalated',
  'needs-clarification': 'Needs input',
  rework: 'Rework',
  'handed-off': 'Handed off',
  errored: 'Errored',
  merged: 'Merged',
  'completed-no-pr': 'Done',
  canceled: 'Canceled',
};

const S = 16; // icon size

/** Dotted circle — queued / new (not started) */
function IconQueued() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle
        cx="8"
        cy="8"
        r="6"
        stroke="var(--color-text-3)"
        strokeWidth="1.5"
        strokeDasharray="2.5 2.5"
      />
    </svg>
  );
}

/** Half-filled circle — in progress / clarifying */
function IconWorking() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--color-accent)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12V2z" fill="var(--color-accent)" />
    </svg>
  );
}

/** Three-quarter circle — captain reviewing (almost done) */
function IconReviewing() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--color-accent)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12A6 6 0 0 1 2 8h6V2z" fill="var(--color-accent)" />
    </svg>
  );
}

/** Half circle orange — rework */
function IconRework() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--color-stale)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12V2z" fill="var(--color-stale)" />
    </svg>
  );
}

/** Open circle — handed off (parked) */
function IconHandedOff() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--color-text-3)" strokeWidth="1.5" />
    </svg>
  );
}

/** Circle with lightning — errored */
function IconErrored() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--color-error)" strokeWidth="1.5" />
      <path
        d="M9 4L6.5 8.5H8.5L7 12"
        stroke="var(--color-error)"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Filled circle with checkmark — merged / completed */
function IconDone() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="7" fill="var(--color-success)" />
      <path
        d="M5.5 8.5l2 2 3.5-4"
        stroke="var(--color-surface-1)"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Circle with X — canceled */
function IconCanceled() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--color-text-4)" strokeWidth="1.5" />
      <path
        d="M6 6l4 4M10 6l-4 4"
        stroke="var(--color-text-4)"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

/** Filled circle green — awaiting review (ready for human) */
function IconAwaitingReview() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="7" fill="var(--color-success)" />
      <circle cx="8" cy="8" r="3" fill="var(--color-surface-1)" />
    </svg>
  );
}

/** Filled circle with ? — human action needed (escalated + needs clarification) */
function IconActionNeeded({ color }: { color: string }) {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="7" fill={color} />
      <path
        d="M7 6.5a1.5 1.5 0 0 1 2.5 1c0 1-1.5 1-1.5 2M8 11.5v.5"
        stroke="var(--color-surface-1)"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

const ICON_MAP: Record<string, () => React.ReactElement> = {
  new: IconQueued,
  queued: IconQueued,
  clarifying: IconWorking,
  'in-progress': IconWorking,
  'captain-reviewing': IconReviewing,
  'captain-merging': IconReviewing,
  'awaiting-review': IconAwaitingReview,
  escalated: () => IconActionNeeded({ color: 'var(--color-error)' }),
  'needs-clarification': () => IconActionNeeded({ color: 'var(--color-needs-human)' }),
  rework: IconRework,
  'handed-off': IconHandedOff,
  errored: IconErrored,
  merged: IconDone,
  'completed-no-pr': IconDone,
  canceled: IconCanceled,
};

export function StatusIcon({ status }: { status: string }): React.ReactElement {
  const Icon = ICON_MAP[status] ?? IconQueued;
  return (
    <span
      className="inline-flex shrink-0 items-center justify-center"
      style={{ width: 16 }}
      title={STATUS_TOOLTIP[status] ?? status}
    >
      <Icon />
    </span>
  );
}

export function ActionBtn({
  label,
  onClick,
  testId,
  disabled,
  pending,
}: {
  label: string;
  onClick: () => void;
  testId?: string;
  disabled?: boolean;
  pending?: boolean;
}): React.ReactElement {
  const isDisabled = disabled || pending;
  return (
    <button
      data-testid={testId}
      onClick={onClick}
      disabled={isDisabled}
      className="shrink-0 rounded px-2 py-0.5 text-[11px] font-medium transition-colors disabled:opacity-40"
      style={{
        background: 'transparent',
        color: 'var(--color-text-2)',
        border: '1px solid var(--color-border-subtle)',
        cursor: isDisabled ? 'default' : 'pointer',
      }}
    >
      {pending ? '...' : label}
    </button>
  );
}

export function OverflowMenu({
  item,
  triggerRef,
  onRework,
  onHandoff,
  onCancel,
  onRetry,
  onAnswer,
  onClose,
}: {
  item: TaskItem;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  onRework: () => void;
  onHandoff: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onAnswer: () => void;
  onClose: () => void;
}): React.ReactElement {
  const menuRef = React.useRef<HTMLDivElement>(null);

  useMountEffect(() => {
    // Click-outside to close
    const handleClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (menuRef.current?.contains(target)) return;
      if (triggerRef.current?.contains(target)) return;
      onClose();
    };
    // Keyboard navigation
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
        triggerRef.current?.focus();
        return;
      }
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      e.preventDefault();
      const items = menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]');
      if (!items?.length) return;
      const active = document.activeElement as HTMLElement;
      const idx = Array.from(items).indexOf(active as HTMLButtonElement);
      const next =
        e.key === 'ArrowDown' ? (idx + 1) % items.length : (idx - 1 + items.length) % items.length;
      items[next].focus();
    };
    document.addEventListener('mousedown', handleClick);
    document.addEventListener('keydown', handleKey);
    // Focus first item on open
    requestAnimationFrame(() => {
      menuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
    });
    return () => {
      document.removeEventListener('mousedown', handleClick);
      document.removeEventListener('keydown', handleKey);
    };
  });

  const menuItem = (label: string, onClick: () => void, danger = false) => (
    <button
      key={label}
      role="menuitem"
      onClick={() => {
        onClick();
        onClose();
      }}
      className="block w-full text-left px-3 py-1.5 text-[12px] transition-colors"
      style={{
        background: 'transparent',
        border: 'none',
        color: danger ? 'var(--color-error)' : 'var(--color-text-1)',
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );

  const showRework = ['awaiting-review', 'handed-off', 'escalated', 'errored'].includes(
    item.status,
  );
  const showHandoff = ['awaiting-review', 'escalated'].includes(item.status);
  const showRetry = item.status === 'errored';
  const showAnswer = item.status === 'needs-clarification';

  return (
    <div
      ref={menuRef}
      role="menu"
      className="absolute right-0 top-full z-50 mt-1 min-w-[140px] rounded-md py-1"
      style={{
        background: 'var(--color-surface-3)',
        border: '1px solid var(--color-border)',
        boxShadow: '0 4px 12px rgba(0,0,0,0.4)',
      }}
    >
      {showRetry && menuItem('Retry', onRetry)}
      {showAnswer && menuItem('Answer', onAnswer)}
      {showRework && menuItem('Rework (new PR)', onRework)}
      {showHandoff && menuItem('Hand off to human', onHandoff)}
      {menuItem('Cancel task', onCancel, true)}
    </div>
  );
}

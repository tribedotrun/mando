import React, { useCallback, useState } from 'react';
import { useScrollIntoViewRef } from '#renderer/global/hooks/useScrollIntoViewRef';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { toast } from 'sonner';
import { useFilteredTasks } from '#renderer/domains/captain/hooks/useFilteredTasks';
import type { TaskItem } from '#renderer/types';
import {
  prLabel,
  prHref,
  prState,
  canMerge,
  canReopen,
  canAskTerminal,
  getErrorMessage,
} from '#renderer/utils';
import { TaskEmptyState } from '#renderer/domains/captain/components/TaskDetails';
import { MergeBtn, PrIcon, MoreIcon } from '#renderer/domains/captain/components/TaskIcons';

import {
  StatusIcon,
  ACTION_LABELS,
  STATUS_TOOLTIP,
  ActionBtn,
  TaskOverflowMenu,
} from '#renderer/domains/captain/components/TaskActions';
import { archiveItem, unarchiveItem } from '#renderer/domains/captain/hooks/useApi';

interface Props {
  selectedIds: Set<number>;
  onToggleSelect: (id: number) => void;
  onToggleSelectAll: (visible: TaskItem[]) => void;
  onMerge: (item: TaskItem) => void;
  onReopen: (item: TaskItem) => void;
  onRework: (item: TaskItem) => void;
  onAsk: (item: TaskItem) => void;
  onAccept: (id: number) => void;
  onHandoff: (id: number) => void;
  onCancel: (id: number) => void;
  onRetry: (id: number) => void;
  onAnswer: (item: TaskItem) => void;
  onOpenDetail?: (item: TaskItem) => void;
  projectFilter?: string | null;
  focusedIndex?: number;
}

export function TaskTable(props: Props): React.ReactElement {
  const {
    selectedIds,
    onToggleSelect,
    onMerge,
    onReopen,
    onRework,
    onAsk,
    onAccept,
    onHandoff,
    onCancel,
    onRetry,
    onAnswer,
    onOpenDetail,
    projectFilter,
    focusedIndex = -1,
  } = props;
  const loading = useTaskStore((s) => s.loading);
  const error = useTaskStore((s) => s.error);
  const items = useFilteredTasks(projectFilter);
  // Ref callback: scroll focused row into view when the DOM node mounts/updates
  const scrollRef = useScrollIntoViewRef();

  if (loading && items.length === 0) {
    return <div className="py-8 text-center text-body text-text-3">Loading...</div>;
  }
  if (error) {
    return <div className="py-8 text-center text-body text-error">{error}</div>;
  }
  if (items.length === 0) {
    return <TaskEmptyState />;
  }

  return (
    <div className="flex flex-col" style={{ gap: 0 }}>
      {items.map((item, idx) => (
        <TaskRow
          key={item.id}
          item={item}
          selected={selectedIds.has(item.id)}
          focused={idx === focusedIndex}
          scrollRef={idx === focusedIndex ? scrollRef : undefined}
          onToggleSelect={() => onToggleSelect(item.id)}
          onMerge={() => onMerge(item)}
          onReopen={() => onReopen(item)}
          onRework={() => onRework(item)}
          onAsk={() => onAsk(item)}
          onAccept={() => onAccept(item.id)}
          onHandoff={() => onHandoff(item.id)}
          onCancel={() => onCancel(item.id)}
          onRetry={() => onRetry(item.id)}
          onAnswer={() => onAnswer(item)}
          onOpenDetail={onOpenDetail ? () => onOpenDetail(item) : undefined}
        />
      ))}

      {/* Table footer */}
      <div
        style={{
          paddingTop: 8,
          paddingInline: 12,
          fontSize: 11,
          fontWeight: 400,
          color: 'var(--color-text-4)',
          letterSpacing: '0.02em',
        }}
      >
        {items.length} tasks
      </div>
    </div>
  );
}

/* ── Single row ── */
interface RowProps {
  item: TaskItem;
  selected: boolean;
  focused: boolean;
  scrollRef?: (node: HTMLElement | null) => void;
  onToggleSelect: () => void;
  onMerge: () => void;
  onReopen: () => void;
  onRework: () => void;
  onAsk: () => void;
  onAccept: () => void;
  onHandoff: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onAnswer: () => void;
  onOpenDetail?: () => void;
}

const TaskRow = React.memo(function TaskRow({
  item,
  selected,
  focused,
  scrollRef,
  ...actions
}: RowProps): React.ReactElement {
  const refetchTasks = useTaskStore((s) => s.fetch);
  const isFinalized =
    item.status === 'merged' || item.status === 'completed-no-pr' || item.status === 'canceled';
  const [menuOpen, setMenuOpen] = useState(false);
  const [archivePending, setArchivePending] = useState(false);

  const handleArchive = useCallback(async () => {
    setArchivePending(true);
    try {
      await archiveItem(item.id);
      refetchTasks();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Archive failed'));
      refetchTasks();
    } finally {
      setArchivePending(false);
    }
  }, [item.id, refetchTasks]);

  const handleUnarchive = useCallback(async () => {
    setArchivePending(true);
    try {
      await unarchiveItem(item.id);
      refetchTasks();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Unarchive failed'));
      refetchTasks();
    } finally {
      setArchivePending(false);
    }
  }, [item.id, refetchTasks]);

  return (
    <div
      ref={scrollRef}
      data-testid="task-row"
      data-focused={focused || undefined}
      className="group relative flex cursor-pointer items-center"
      style={{
        paddingBlock: 8,
        paddingInline: 12,
        gap: 10,
        background: selected ? 'var(--color-accent-wash)' : 'var(--color-surface-1)',
        opacity: isFinalized ? 0.55 : 1,
        borderBottom: '1px solid var(--color-border)',
        outline: focused ? '2px solid var(--color-accent)' : 'none',
        outlineOffset: -2,
        zIndex: menuOpen ? 20 : undefined,
      }}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('[data-actions]')) return;
        actions.onOpenDetail?.();
      }}
    >
      {/* Col: checkbox, overlays the status icon on hover/select */}
      <span className="status-icon-wrapper relative shrink-0" style={{ width: 16, height: 16 }}>
        <span
          className={`absolute inset-0 flex items-center justify-center transition-opacity ${selected ? 'opacity-0' : 'group-hover:opacity-0'}`}
        >
          <StatusIcon status={item.status} />
        </span>
        <input
          type="checkbox"
          checked={selected}
          aria-label={`Select "${item.title}"`}
          onChange={actions.onToggleSelect}
          onClick={(e) => e.stopPropagation()}
          className={`absolute inset-0 m-auto cursor-pointer transition-opacity group-hover:opacity-100 ${selected ? 'opacity-100' : 'opacity-0'}`}
          style={{
            width: 14,
            height: 14,
            borderRadius: 4,
            accentColor: 'var(--color-accent)',
            zIndex: 1,
          }}
        />
        <span
          className="status-tooltip pointer-events-none absolute left-1/2 -translate-x-1/2 whitespace-nowrap"
          style={{
            top: -30,
            fontSize: 11,
            fontWeight: 500,
            color: 'var(--color-text-1)',
            background: 'var(--color-surface-3)',
            border: '1px solid var(--color-border)',
            padding: '2px 8px',
            borderRadius: 4,
            opacity: 0,
            transition: 'opacity 150ms',
            zIndex: 50,
            boxShadow: '0 2px 8px rgba(0,0,0,0.3)',
          }}
        >
          {STATUS_TOOLTIP[item.status] ?? item.status}
        </span>
      </span>

      {/* Col: title + badges, title truncates, badges never compress */}
      <span
        className="flex min-w-0 flex-1 items-center"
        style={{
          fontSize: 14,
          lineHeight: '18px',
          gap: 6,
        }}
      >
        <span
          className="min-w-0 truncate"
          style={{ color: isFinalized ? 'var(--color-text-3)' : 'var(--color-text-1)' }}
          title={item.title}
        >
          {ACTION_LABELS[item.status] && (
            <span
              style={{
                fontSize: 12,
                fontWeight: 500,
                color: ACTION_LABELS[item.status].color,
                marginRight: 6,
              }}
            >
              {ACTION_LABELS[item.status].label}
              {' \u00b7 '}
            </span>
          )}
          {item.session_ids?.ask && (
            <span
              style={{
                fontSize: 11,
                fontWeight: 500,
                color: 'var(--color-accent)',
                marginRight: 6,
                opacity: 0.85,
              }}
            >
              Q&A
              {' \u00b7 '}
            </span>
          )}
          {item.title}
        </span>
        {item.pr && (item.github_repo || item.project) && (
          <a
            href={prHref(item.pr, (item.github_repo ?? item.project)!)}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex shrink-0 items-center no-underline hover:underline"
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: 11,
              color: 'var(--color-text-3)',
              background: 'var(--color-surface-3)',
              padding: '0 4px',
              borderRadius: 4,
              gap: 3,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <PrIcon state={prState(item.status)} />
            {prLabel(item.pr)}
          </a>
        )}
      </span>

      {/* Actions, inline flex item, hidden until hover via CSS */}
      <div
        data-actions
        data-menu-open={menuOpen || undefined}
        className="action-zone shrink-0 items-center"
        style={{
          display: 'flex',
          gap: 6,
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {canMerge(item) && <MergeBtn onClick={actions.onMerge} />}
        {item.status === 'awaiting-review' && !item.pr && (
          <ActionBtn label="Accept" onClick={actions.onAccept} testId="accept-btn" />
        )}
        {item.status === 'needs-clarification' && (
          <ActionBtn label="Answer" onClick={actions.onAnswer} testId="answer-btn" />
        )}
        {canReopen(item) && (
          <ActionBtn label="Reopen" onClick={actions.onReopen} testId="reopen-btn" />
        )}
        {isFinalized &&
          (item.archived_at ? (
            <ActionBtn
              label="Unarchive"
              onClick={handleUnarchive}
              testId="unarchive-btn"
              pending={archivePending}
            />
          ) : (
            <ActionBtn
              label="Archive"
              onClick={handleArchive}
              testId="archive-btn"
              pending={archivePending}
            />
          ))}
        {canAskTerminal(item) && <ActionBtn label="Ask" onClick={actions.onAsk} />}
        {!isFinalized && (
          <TaskOverflowMenu
            item={item}
            open={menuOpen}
            onOpenChange={setMenuOpen}
            onRework={actions.onRework}
            onHandoff={actions.onHandoff}
            onCancel={actions.onCancel}
            onRetry={actions.onRetry}
            onAnswer={actions.onAnswer}
          >
            <button
              aria-label="More actions"
              className="flex shrink-0 items-center justify-center rounded transition-colors"
              style={{
                width: 24,
                height: 24,
                background: 'transparent',
                color: 'var(--color-text-2)',
                border: '1px solid var(--color-border-subtle)',
                cursor: 'pointer',
              }}
            >
              <MoreIcon />
            </button>
          </TaskOverflowMenu>
        )}
      </div>
    </div>
  );
});

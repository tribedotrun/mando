import React, { useMemo, useState } from 'react';
import {
  type ColumnDef,
  type SortingState,
  type RowSelectionState,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
} from '@tanstack/react-table';
import { useScrollIntoViewRef } from '#renderer/global/hooks/useScrollIntoViewRef';
import { useTaskList } from '#renderer/hooks/queries';
import { useFilteredTasks } from '#renderer/domains/captain/hooks/useFilteredTasks';
import type { TaskItem } from '#renderer/types';
import { TaskEmptyState } from '#renderer/domains/captain/components/TaskDetails';
import { TaskRow, type TaskRowCallbacks } from '#renderer/domains/captain/components/TaskRow';
import { Skeleton } from '#renderer/components/ui/skeleton';

interface Props {
  selectedIds: Set<number>;
  onToggleSelect: (id: number) => void;
  onMerge: (item: TaskItem) => void;
  onReopen: (item: TaskItem) => void;
  onRework: (item: TaskItem) => void;
  onAsk: (item: TaskItem) => void;
  onAccept: (id: number) => void;
  acceptPendingId?: number | null;
  onHandoff: (id: number) => void;
  onCancel: (id: number) => void;
  onRetry: (id: number) => void;
  onAnswer: (item: TaskItem) => void;
  onOpenDetail?: (item: TaskItem) => void;
  projectFilter?: string | null;
  focusedIndex?: number;
}

/** Single invisible column -- the entire row is rendered by TaskRow via renderRow */
const columns: ColumnDef<TaskItem, unknown>[] = [
  {
    id: 'task',
    accessorFn: (row) => row.title,
    header: () => null,
    enableSorting: false,
  },
];

export function TaskTable(props: Props): React.ReactElement {
  const {
    selectedIds,
    onToggleSelect,
    onMerge,
    onReopen,
    onRework,
    onAsk,
    onAccept,
    acceptPendingId,
    onHandoff,
    onCancel,
    onRetry,
    onAnswer,
    onOpenDetail,
    projectFilter,
    focusedIndex = -1,
  } = props;
  const { isLoading: loading, error: queryError } = useTaskList();
  const error = queryError ? String(queryError) : null;
  const items = useFilteredTasks(projectFilter);
  const scrollRef = useScrollIntoViewRef();

  const [sorting, setSorting] = useState<SortingState>([]);

  // Convert Set<number> to TanStack's RowSelectionState (record of rowId -> boolean)
  const rowSelection = useMemo<RowSelectionState>(() => {
    const sel: RowSelectionState = {};
    for (const id of selectedIds) sel[String(id)] = true;
    return sel;
  }, [selectedIds]);

  const callbacks = useMemo<TaskRowCallbacks>(
    () => ({
      onToggleSelect,
      onMerge,
      onReopen,
      onRework,
      onAsk,
      onAccept,
      acceptPendingId,
      onHandoff,
      onCancel,
      onRetry,
      onAnswer,
      onOpenDetail,
    }),
    [
      onToggleSelect,
      onMerge,
      onReopen,
      onRework,
      onAsk,
      onAccept,
      acceptPendingId,
      onHandoff,
      onCancel,
      onRetry,
      onAnswer,
      onOpenDetail,
    ],
  );

  const table = useReactTable({
    data: items,
    columns,
    state: { sorting, rowSelection },
    onSortingChange: setSorting,
    // Selection is driven externally via selectedIds prop, not TanStack state
    enableRowSelection: true,
    getRowId: (row) => String(row.id),
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  if (loading && items.length === 0) {
    return (
      <div className="space-y-3 px-3 py-8">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    );
  }
  if (error) {
    return <div className="py-8 text-center text-body text-destructive">{error}</div>;
  }
  if (items.length === 0) {
    return <TaskEmptyState />;
  }

  const rows = table.getRowModel().rows;

  return (
    <div className="flex flex-col gap-px">
      {rows.map((row, idx) => (
        <TaskRow
          key={row.id}
          row={row}
          focused={idx === focusedIndex}
          scrollRef={idx === focusedIndex ? scrollRef : undefined}
          callbacks={callbacks}
        />
      ))}

      {/* Table footer */}
      <div className="px-3 pt-2 text-[11px] font-normal tracking-wide text-text-4">
        {items.length} tasks
      </div>
    </div>
  );
}

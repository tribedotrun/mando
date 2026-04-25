import React, { useCallback, useRef, useState } from 'react';
import { FileText } from 'lucide-react';
import type { ScoutItem } from '#renderer/global/types';
import { useScoutStatusUpdate } from '#renderer/domains/scout/runtime/hooks';
import { scoutCommandForStatus } from '#renderer/domains/scout/runtime/useScoutPage';
import type { ScoutUserSettableStatus } from '#renderer/domains/scout/service/researchHelpers';
import { useScrollIntoViewRef } from '#renderer/global/runtime/useScrollIntoViewRef';
import { EmptyState } from '#renderer/domains/scout/ui/EmptyState';
import { Table, TableBody } from '#renderer/global/ui/primitives/table';
import { ScoutTableRow } from '#renderer/domains/scout/ui/ScoutTableRow';

export interface ScoutTableCallbacks {
  onToggleSelect: (id: number) => void;
  onSelect: (id: number) => void;
}

interface Props {
  items: ScoutItem[];
  selectedIds: Set<number>;
  callbacks: ScoutTableCallbacks;
  focusedIndex?: number;
}

export function ScoutTable({
  items,
  selectedIds,
  callbacks,
  focusedIndex = -1,
}: Props): React.ReactElement {
  const { onToggleSelect, onSelect } = callbacks;
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const statusMut = useScoutStatusUpdate();
  const listRef = useRef<HTMLDivElement>(null);

  // Scroll focused row into view via ref callback
  const scrollRef = useScrollIntoViewRef();

  const toggleExpand = useCallback((id: number) => {
    setExpandedId((prev) => (prev === id ? null : id));
  }, []);

  const handleStatusChange = (id: number, status: ScoutUserSettableStatus) => {
    statusMut.mutate(
      { id, command: scoutCommandForStatus(status) },
      {
        onSettled: () => setEditingId(null),
      },
    );
  };

  if (items.length === 0) {
    return (
      <div data-testid="scout-table">
        <EmptyState
          icon={<FileText size={48} color="var(--text-4)" strokeWidth={1.5} />}
          heading="No scout items yet"
          description="Add a URL to start building your scout feed."
        />
      </div>
    );
  }

  return (
    <div ref={listRef} data-testid="scout-table">
      <Table>
        <TableBody>
          {items.map((item, idx) => (
            <ScoutTableRow
              key={item.id}
              item={item}
              isSelected={selectedIds.has(item.id)}
              isFocused={idx === focusedIndex}
              isExpanded={expandedId === item.id}
              isEditing={editingId === item.id}
              scrollRef={idx === focusedIndex ? scrollRef : undefined}
              callbacks={{
                onToggleSelect,
                onSelect,
                onToggleExpand: toggleExpand,
                onStatusChange: handleStatusChange,
                onStartEdit: setEditingId,
              }}
            />
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

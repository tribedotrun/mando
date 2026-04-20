import React from 'react';
import { ChevronRight } from 'lucide-react';
import type { ScoutItem } from '#renderer/global/types';
import { useScoutTableRow } from '#renderer/domains/scout/runtime/useScoutTableRow';
import { Badge } from '#renderer/global/ui/badge';
import { TableRow, TableCell } from '#renderer/global/ui/table';
import { Checkbox } from '#renderer/global/ui/checkbox';
import { ExpandedSummaryRow, StatusCell } from '#renderer/domains/scout/ui/ScoutTableRowParts';

export interface ScoutTableRowCallbacks {
  onToggleSelect: (id: number) => void;
  onSelect: (id: number) => void;
  onToggleExpand: (id: number) => void;
  onStatusChange: (id: number, status: string) => void;
  onStartEdit: (id: number) => void;
}

interface Props {
  item: ScoutItem;
  isSelected: boolean;
  isFocused: boolean;
  isExpanded: boolean;
  isEditing: boolean;
  scrollRef: React.RefCallback<HTMLTableRowElement> | undefined;
  callbacks: ScoutTableRowCallbacks;
}

export function ScoutTableRow({
  item,
  isSelected,
  isFocused,
  isExpanded,
  isEditing,
  scrollRef,
  callbacks,
}: Props): React.ReactElement {
  const { onToggleSelect, onSelect, onToggleExpand, onStatusChange, onStartEdit } = callbacks;
  const hasSummary = !!item.has_summary;
  const { summaryContent, summaryLoading, summaryError, badge, domain, statusVariant } =
    useScoutTableRow({ item, isExpanded });

  return (
    <React.Fragment>
      <TableRow
        ref={isFocused ? scrollRef : undefined}
        data-testid="scout-row"
        data-focused={isFocused || undefined}
        data-state={isSelected ? 'selected' : undefined}
        className={`cursor-pointer ${isFocused ? 'outline outline-2 outline-ring -outline-offset-2' : ''}`}
        onClick={() => onSelect(item.id)}
      >
        <TableCell>
          <div className="flex items-center gap-1.5">
            <Checkbox
              checked={isSelected}
              onCheckedChange={() => onToggleSelect(item.id)}
              onClick={(e) => e.stopPropagation()}
              aria-label={`Select ${item.title || domain || 'Untitled'}`}
              className="size-3.5"
            />
            {hasSummary && (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleExpand(item.id);
                }}
                aria-expanded={isExpanded}
                aria-controls={`scout-summary-${item.id}`}
                aria-label={isExpanded ? 'Collapse summary' : 'Expand summary'}
                className="rounded p-0.5 text-muted-foreground hover:text-foreground"
              >
                <ChevronRight
                  size={11}
                  className={`shrink-0 transition-transform duration-150 ${isExpanded ? 'rotate-90' : ''}`}
                />
              </button>
            )}
          </div>
        </TableCell>

        <TableCell title={item.url}>
          <span
            role="link"
            tabIndex={0}
            onClick={(e) => {
              e.stopPropagation();
              onSelect(item.id);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                e.stopPropagation();
                onSelect(item.id);
              }
            }}
            className="block cursor-pointer truncate text-[13px] text-foreground"
          >
            {item.title || (item.status === 'pending' ? 'Pending...' : domain || 'Untitled')}
          </span>
          {domain && (
            <span className="block truncate text-[11px] text-muted-foreground">{domain}</span>
          )}
        </TableCell>

        <TableCell className="text-center">
          {badge.label && (
            <Badge variant={badge.variant} className="text-[11px]">
              {badge.label}
            </Badge>
          )}
        </TableCell>

        <TableCell className="text-center" onClick={(e) => e.stopPropagation()}>
          <StatusCell
            itemId={item.id}
            status={item.status}
            isEditing={isEditing}
            statusVariant={statusVariant}
            onStatusChange={onStatusChange}
            onStartEdit={onStartEdit}
          />
        </TableCell>
      </TableRow>

      {/* Expanded summary */}
      {isExpanded && (
        <ExpandedSummaryRow
          itemId={item.id}
          isExpanded={isExpanded}
          summaryLoading={summaryLoading}
          summaryContent={summaryContent}
          summaryError={summaryError}
        />
      )}
    </React.Fragment>
  );
}

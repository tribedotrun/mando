import React from 'react';
import Markdown from 'react-markdown';
import { ChevronRight } from 'lucide-react';
import type { ScoutItem } from '#renderer/global/types';
import { useScoutItem } from '#renderer/domains/scout/runtime/hooks';
import {
  USER_SETTABLE_STATUSES,
  SCOUT_STATUS_VARIANT,
  SCOUT_TYPE_BADGE,
  scoutItemDomain,
} from '#renderer/domains/scout/service/researchHelpers';
import { Badge } from '#renderer/global/ui/badge';
import { TableRow, TableCell } from '#renderer/global/ui/table';
import { Collapsible, CollapsibleContent } from '#renderer/global/ui/collapsible';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { Checkbox } from '#renderer/global/ui/checkbox';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '#renderer/global/ui/select';

const USER_SETTABLE = USER_SETTABLE_STATUSES as readonly string[];

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
  const summaryQuery = useScoutItem(item.id, { enabled: isExpanded && hasSummary });
  const summaryContent = summaryQuery.data?.summary;
  const summaryLoading = summaryQuery.isLoading;
  const summaryError = summaryQuery.error ? String(summaryQuery.error) : undefined;
  const badge = SCOUT_TYPE_BADGE[item.item_type ?? 'other'] ?? SCOUT_TYPE_BADGE.other;
  const domain = scoutItemDomain(item);
  const statusVariant = SCOUT_STATUS_VARIANT[item.status] ?? 'outline';

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
          {item.status === 'processed' ? null : isEditing && USER_SETTABLE.includes(item.status) ? (
            <Select
              value={item.status}
              onValueChange={(v) => {
                onStatusChange(item.id, v);
              }}
              onOpenChange={(open) => {
                if (!open) onStartEdit(-1);
              }}
              open
            >
              <SelectTrigger size="sm" className="h-6 text-[11px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {USER_SETTABLE.map((s) => (
                  <SelectItem key={s} value={s}>
                    {s}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : USER_SETTABLE.includes(item.status) ? (
            <Badge
              variant={statusVariant}
              className="cursor-pointer text-[11px]"
              onClick={() => onStartEdit(item.id)}
              aria-label={`Change status, currently ${item.status}`}
            >
              {item.status}
            </Badge>
          ) : (
            <Badge variant={statusVariant} className="text-[11px]">
              {item.status}
            </Badge>
          )}
        </TableCell>
      </TableRow>

      {/* Expanded summary */}
      {isExpanded && (
        <TableRow className="hover:bg-transparent">
          <TableCell colSpan={4} className="p-0">
            <Collapsible open={isExpanded}>
              <CollapsibleContent id={`scout-summary-${item.id}`}>
                {summaryLoading ? (
                  <div className="space-y-2 px-10 py-3">
                    <Skeleton className="h-4 w-3/4" />
                    <Skeleton className="h-4 w-1/2" />
                    <Skeleton className="h-4 w-2/3" />
                  </div>
                ) : summaryContent ? (
                  <div className="prose-scout bg-muted px-10 py-3">
                    <Markdown>{summaryContent}</Markdown>
                  </div>
                ) : summaryError ? (
                  <div className="px-10 py-3 text-xs text-destructive">{summaryError}</div>
                ) : null}
              </CollapsibleContent>
            </Collapsible>
          </TableCell>
        </TableRow>
      )}
    </React.Fragment>
  );
}

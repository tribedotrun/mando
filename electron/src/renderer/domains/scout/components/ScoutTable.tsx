import React, { useCallback, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import { ChevronRight, FileText } from 'lucide-react';
import type { ScoutItem } from '#renderer/types';
import { fetchScoutItem, updateScoutStatus } from '#renderer/domains/scout/hooks/useApi';
import { useScrollIntoViewRef } from '#renderer/global/hooks/useScrollIntoViewRef';
import { EmptyState } from '#renderer/global/components/EmptyState';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';
import { Badge } from '#renderer/components/ui/badge';
import { Table, TableBody, TableRow, TableCell } from '#renderer/components/ui/table';
import { Collapsible, CollapsibleContent } from '#renderer/components/ui/collapsible';
import { Skeleton } from '#renderer/components/ui/skeleton';
import { Checkbox } from '#renderer/components/ui/checkbox';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '#renderer/components/ui/select';

const STATUS_VARIANT: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  pending: 'outline',
  fetched: 'secondary',
  processed: 'default',
  saved: 'secondary',
  archived: 'outline',
  error: 'destructive',
};

const TYPE_BADGE: Record<string, { label: string; variant: 'outline' }> = {
  github: { label: 'GH', variant: 'outline' },
  youtube: { label: 'YT', variant: 'outline' },
  arxiv: { label: 'arXiv', variant: 'outline' },
  blog: { label: 'blog', variant: 'outline' },
  other: { label: '', variant: 'outline' },
};

const USER_SETTABLE = ['pending', 'processed', 'saved', 'archived'];

export interface ScoutTableCallbacks {
  onToggleSelect: (id: number) => void;
  onSelect: (id: number) => void;
  onRefresh: () => void;
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
  const { onToggleSelect, onSelect, onRefresh } = callbacks;
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [summaryCache, setSummaryCache] = useState<Record<number, string>>({});
  const [summaryErrors, setSummaryErrors] = useState<Record<number, string>>({});
  const [summaryLoading, setSummaryLoading] = useState<Record<number, boolean>>({});
  const [editingId, setEditingId] = useState<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Scroll focused row into view via ref callback
  const scrollRef = useScrollIntoViewRef();

  const toggleExpand = useCallback(
    async (id: number, hasSummary: boolean) => {
      if (expandedId === id) {
        setExpandedId(null);
        return;
      }
      setExpandedId(id);
      if (!hasSummary || summaryCache[id]) return;
      setSummaryLoading((prev) => ({ ...prev, [id]: true }));
      try {
        const data = await fetchScoutItem(id);
        if (data.summary) {
          setSummaryCache((c) => ({ ...c, [id]: data.summary! }));
          setSummaryErrors((e) => {
            if (!(id in e)) return e;
            const next = { ...e };
            delete next[id];
            return next;
          });
        }
      } catch (err) {
        const msg = getErrorMessage(err, 'Failed to load summary');
        log.warn('[Scout] failed to fetch scout item summary', { id, err });
        setSummaryErrors((e) => ({ ...e, [id]: msg }));
      } finally {
        setSummaryLoading((prev) => ({ ...prev, [id]: false }));
      }
    },
    [expandedId, summaryCache],
  );

  const handleStatusChange = (id: number, status: string) => {
    void (async () => {
      try {
        await updateScoutStatus(id, status);
        onRefresh();
      } catch (err) {
        toast.error(`Status update failed: ${getErrorMessage(err, 'unknown error')}`);
      }
      setEditingId(null);
    })();
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
          {items.map((item, idx) => {
            const isExpanded = expandedId === item.id;
            const hasSummary = !!item.has_summary;
            const sel = selectedIds.has(item.id);
            const isFocused = idx === focusedIndex;
            const badge = TYPE_BADGE[item.item_type ?? 'other'] ?? TYPE_BADGE.other;
            const domain =
              item.source_name || (item.url ? new URL(item.url).hostname.replace('www.', '') : '');
            const statusVariant = STATUS_VARIANT[item.status] ?? 'outline';

            return (
              <React.Fragment key={item.id}>
                <TableRow
                  ref={isFocused ? scrollRef : undefined}
                  data-testid="scout-row"
                  data-focused={isFocused || undefined}
                  data-state={sel ? 'selected' : undefined}
                  className={`cursor-pointer ${isFocused ? 'outline outline-2 outline-ring -outline-offset-2' : ''}`}
                  onClick={() => onSelect(item.id)}
                >
                  <TableCell>
                    <div className="flex items-center gap-1.5">
                      <Checkbox
                        checked={sel}
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
                            void toggleExpand(item.id, hasSummary);
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
                      {item.title ||
                        (item.status === 'pending' ? 'Pending...' : domain || 'Untitled')}
                    </span>
                    {domain && (
                      <span className="block truncate text-[11px] text-muted-foreground">
                        {domain}
                      </span>
                    )}
                  </TableCell>

                  <TableCell className="text-center">
                    {badge.label && (
                      <Badge variant={badge.variant} className="text-[10px]">
                        {badge.label}
                      </Badge>
                    )}
                  </TableCell>

                  <TableCell className="text-center" onClick={(e) => e.stopPropagation()}>
                    {editingId === item.id && USER_SETTABLE.includes(item.status) ? (
                      <Select
                        value={item.status}
                        onValueChange={(v) => {
                          handleStatusChange(item.id, v);
                          setEditingId(null);
                        }}
                        onOpenChange={(open) => {
                          if (!open) setEditingId(null);
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
                        className="cursor-pointer text-[10px]"
                        onClick={() => setEditingId(item.id)}
                        aria-label={`Change status, currently ${item.status}`}
                      >
                        {item.status}
                      </Badge>
                    ) : (
                      <Badge variant={statusVariant} className="text-[10px]">
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
                          {summaryLoading[item.id] ? (
                            <div className="space-y-2 px-10 py-3">
                              <Skeleton className="h-4 w-3/4" />
                              <Skeleton className="h-4 w-1/2" />
                              <Skeleton className="h-4 w-2/3" />
                            </div>
                          ) : summaryCache[item.id] ? (
                            <div className="prose-scout bg-muted px-10 py-3">
                              <Markdown>{summaryCache[item.id]}</Markdown>
                            </div>
                          ) : summaryErrors[item.id] ? (
                            <div className="px-10 py-3 text-xs text-destructive">
                              {summaryErrors[item.id]}
                            </div>
                          ) : null}
                        </CollapsibleContent>
                      </Collapsible>
                    </TableCell>
                  </TableRow>
                )}
              </React.Fragment>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}

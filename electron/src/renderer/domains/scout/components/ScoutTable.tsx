import React, { useCallback, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import { FileText } from 'lucide-react';
import type { ScoutItem } from '#renderer/types';
import { fetchScoutItem, updateScoutStatus } from '#renderer/domains/scout/hooks/useApi';
import { useScrollIntoViewRef } from '#renderer/global/hooks/useScrollIntoViewRef';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';
import { Badge } from '#renderer/components/ui/badge';
import { Button } from '#renderer/components/ui/button';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '#renderer/components/ui/table';
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

const TYPE_BADGE: Record<
  string,
  { label: string; variant: 'default' | 'secondary' | 'outline' | 'destructive' }
> = {
  github: { label: 'GH', variant: 'default' },
  youtube: { label: 'YT', variant: 'destructive' },
  arxiv: { label: 'arXiv', variant: 'default' },
  blog: { label: 'blog', variant: 'secondary' },
  other: { label: '', variant: 'outline' },
};

const USER_SETTABLE = ['pending', 'processed', 'saved', 'archived'];

interface Props {
  items: ScoutItem[];
  selectedIds: Set<number>;
  onToggleSelect: (id: number) => void;
  onSelect: (id: number) => void;
  onRefresh: () => void;
  focusedIndex?: number;
}

export function ScoutTable({
  items,
  selectedIds,
  onToggleSelect,
  onSelect,
  onRefresh,
  focusedIndex = -1,
}: Props): React.ReactElement {
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

  const handleStatusChange = async (id: number, status: string) => {
    try {
      await updateScoutStatus(id, status);
      onRefresh();
    } catch (err) {
      toast.error(`Status update failed: ${getErrorMessage(err, 'unknown error')}`);
    }
    setEditingId(null);
  };

  if (items.length === 0) {
    return (
      <div data-testid="scout-table" className="flex flex-col items-center justify-center py-16">
        <FileText size={48} color="var(--text-4)" strokeWidth={1.5} className="mb-4" />
        <span className="text-subheading mb-1 text-muted-foreground">No scout items yet</span>
        <span className="text-body mb-4 text-text-3">
          Add a URL to start building your scout feed.
        </span>
      </div>
    );
  }

  return (
    <div ref={listRef} data-testid="scout-table">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="w-8" />
            <TableHead className="flex-1">Title</TableHead>
            <TableHead className="w-16 text-center">Type</TableHead>
            <TableHead className="w-20 text-center">Status</TableHead>
            <TableHead className="w-24 text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
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
                  className={`cursor-pointer ${isFocused ? 'outline outline-2 outline-primary -outline-offset-2' : ''}`}
                  onClick={() => hasSummary && toggleExpand(item.id, hasSummary)}
                >
                  <TableCell>
                    <Checkbox
                      checked={sel}
                      onCheckedChange={() => onToggleSelect(item.id)}
                      onClick={(e) => e.stopPropagation()}
                      aria-label={`Select ${item.title || 'Untitled'}`}
                      className="size-3.5"
                    />
                  </TableCell>

                  <TableCell>
                    <Button
                      variant="ghost"
                      onClick={(e) => {
                        e.stopPropagation();
                        onSelect(item.id);
                      }}
                      className="h-auto min-w-0 flex-1 flex-col items-start p-0 text-left hover:bg-transparent hover:underline"
                      title={item.url}
                    >
                      <span className="block truncate text-[13px] text-foreground">
                        {item.title || (item.status === 'pending' ? 'Pending...' : 'Untitled')}
                      </span>
                      {domain && (
                        <span className="block truncate text-[11px] text-muted-foreground">
                          {domain}
                        </span>
                      )}
                    </Button>
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

                  <TableCell className="text-right" onClick={(e) => e.stopPropagation()}>
                    {['processed', 'saved', 'archived'].includes(item.status) && (
                      <Button variant="ghost" size="xs" onClick={() => onSelect(item.id)}>
                        Read
                      </Button>
                    )}
                  </TableCell>
                </TableRow>

                {/* Expanded summary */}
                {isExpanded && (
                  <TableRow className="hover:bg-transparent">
                    <TableCell colSpan={5} className="p-0">
                      <Collapsible open={isExpanded}>
                        <CollapsibleContent>
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

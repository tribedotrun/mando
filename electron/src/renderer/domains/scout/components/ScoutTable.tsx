import React, { useCallback, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import { FileText } from 'lucide-react';
import type { ScoutItem } from '#renderer/types';
import { fetchScoutItem, updateScoutStatus } from '#renderer/domains/scout/hooks/useApi';
import { useScrollIntoViewRef } from '#renderer/global/hooks/useScrollIntoViewRef';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';

const STATUS_STYLES: Record<string, { bg: string; color: string }> = {
  pending: { bg: 'transparent', color: 'var(--color-text-3)' },
  fetched: { bg: 'var(--color-review-bg)', color: 'var(--color-accent)' },
  processed: { bg: 'var(--color-success-bg)', color: 'var(--color-success)' },
  saved: { bg: 'var(--color-stale-bg)', color: 'var(--color-stale)' },
  archived: { bg: 'transparent', color: 'var(--color-text-4)' },
  error: { bg: 'var(--color-error-bg)', color: 'var(--color-error)' },
};

const TYPE_BADGE: Record<string, { label: string; color: string }> = {
  github: { label: 'GH', color: 'var(--color-accent)' },
  youtube: { label: 'YT', color: 'var(--color-error)' },
  arxiv: { label: 'arXiv', color: 'var(--color-accent)' },
  blog: { label: 'blog', color: 'var(--color-success)' },
  other: { label: '', color: 'var(--color-text-4)' },
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
        <FileText size={48} color="var(--color-text-4)" strokeWidth={1.5} className="mb-4" />
        <span className="text-subheading mb-1 text-text-2">No scout items yet</span>
        <span className="text-body mb-4 text-text-3">
          Add a URL to start building your scout feed.
        </span>
      </div>
    );
  }

  return (
    <div ref={listRef} data-testid="scout-table" className="flex flex-col" style={{ gap: 1 }}>
      {items.map((item, idx) => {
        const sc = STATUS_STYLES[item.status] ?? STATUS_STYLES.pending;
        const isExpanded = expandedId === item.id;
        const hasSummary = !!item.has_summary;
        const sel = selectedIds.has(item.id);
        const isFocused = idx === focusedIndex;
        const badge = TYPE_BADGE[item.item_type ?? 'other'] ?? TYPE_BADGE.other;
        const domain =
          item.source_name || (item.url ? new URL(item.url).hostname.replace('www.', '') : '');

        return (
          <React.Fragment key={item.id}>
            {/* Main row — single-line, actions always visible */}
            <div
              ref={isFocused ? scrollRef : undefined}
              data-testid="scout-row"
              data-focused={isFocused || undefined}
              role="button"
              aria-label={`Scout item: ${item.title || 'Untitled'}`}
              className="group flex cursor-pointer items-center"
              style={{
                paddingBlock: 8,
                paddingInline: 12,
                gap: 12,
                background: sel ? 'var(--color-accent-wash)' : 'var(--color-surface-1)',
                borderRadius: 'var(--radius-row)',
                outline: isFocused ? '2px solid var(--color-accent)' : 'none',
                outlineOffset: -2,
              }}
              onClick={() => hasSummary && toggleExpand(item.id, hasSummary)}
            >
              <input
                type="checkbox"
                checked={sel}
                onChange={() => onToggleSelect(item.id)}
                onClick={(e) => e.stopPropagation()}
                aria-label={`Select ${item.title || 'Untitled'}`}
                style={{
                  width: 14,
                  height: 14,
                  borderRadius: 4,
                  flexShrink: 0,
                  accentColor: 'var(--color-accent)',
                }}
              />

              {/* Title + source */}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onSelect(item.id);
                }}
                className="min-w-0 flex-1 text-left hover:underline"
                style={{
                  background: 'transparent',
                  border: 'none',
                  cursor: 'pointer',
                  padding: 0,
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 2,
                }}
                title={item.url}
              >
                <span className="truncate text-[13px] text-text-1">
                  {item.title || (item.status === 'pending' ? 'Pending...' : 'Untitled')}
                </span>
                {domain && (
                  <span className="truncate" style={{ fontSize: 11, color: 'var(--color-text-3)' }}>
                    {domain}
                  </span>
                )}
              </button>

              {/* Type badge */}
              <span className="shrink-0 text-center" style={{ width: 64 }}>
                {badge.label && (
                  <span
                    className="text-label inline-block"
                    style={{
                      background: `${badge.color}1A`,
                      color: badge.color,
                      padding: '2px 6px',
                      borderRadius: 4,
                    }}
                  >
                    {badge.label}
                  </span>
                )}
              </span>

              {/* Status */}
              <div
                className="shrink-0 text-center"
                style={{ width: 80 }}
                onClick={(e) => e.stopPropagation()}
              >
                {editingId === item.id && USER_SETTABLE.includes(item.status) ? (
                  <select
                    className="rounded text-[11px]"
                    style={{
                      background: 'var(--color-surface-3)',
                      color: 'var(--color-text-1)',
                      border: '1px solid var(--color-border)',
                    }}
                    value={item.status}
                    onChange={(e) => handleStatusChange(item.id, e.target.value)}
                    onBlur={() => setEditingId(null)}
                    autoFocus
                  >
                    {USER_SETTABLE.map((s) => (
                      <option key={s} value={s}>
                        {s}
                      </option>
                    ))}
                  </select>
                ) : USER_SETTABLE.includes(item.status) ? (
                  <button
                    className="inline-block cursor-pointer appearance-none border-0 bg-transparent font-medium"
                    style={{
                      background: sc.bg,
                      color: sc.color,
                      fontSize: 11,
                      padding: '2px 6px',
                      borderRadius: 4,
                    }}
                    onClick={() => setEditingId(item.id)}
                    aria-label={`Change status, currently ${item.status}`}
                  >
                    {item.status}
                  </button>
                ) : (
                  <span
                    className="inline-block font-medium"
                    style={{
                      background: sc.bg,
                      color: sc.color,
                      fontSize: 11,
                      padding: '2px 6px',
                      borderRadius: 4,
                    }}
                  >
                    {item.status}
                  </span>
                )}
              </div>

              {/* Actions — always visible */}
              <div
                className="flex shrink-0 items-center justify-end"
                style={{ gap: 8, width: 96 }}
                onClick={(e) => e.stopPropagation()}
              >
                {['processed', 'saved', 'archived'].includes(item.status) && (
                  <Btn label="Read" color="var(--color-text-2)" onClick={() => onSelect(item.id)} />
                )}
              </div>
            </div>

            {/* Expanded summary */}
            {isExpanded && summaryCache[item.id] && (
              <div
                className="px-10 py-3 prose-scout"
                style={{
                  background: 'var(--color-surface-2)',
                  borderBottom: '1px solid var(--color-border-subtle)',
                }}
              >
                <Markdown>{summaryCache[item.id]}</Markdown>
              </div>
            )}
            {isExpanded && !summaryCache[item.id] && summaryErrors[item.id] && (
              <div
                className="text-caption px-10 py-3"
                style={{
                  background: 'var(--color-error-bg)',
                  color: 'var(--color-error)',
                  borderBottom: '1px solid var(--color-border-subtle)',
                }}
              >
                {summaryErrors[item.id]}
              </div>
            )}
          </React.Fragment>
        );
      })}
    </div>
  );
}

function Btn({
  label,
  color,
  onClick,
  primary,
}: {
  label: string;
  color: string;
  onClick: () => void;
  primary?: boolean;
}): React.ReactElement {
  return (
    <button
      onClick={onClick}
      className="text-[11px] font-medium transition-colors"
      style={{
        color: primary ? 'var(--color-bg)' : color,
        background: primary ? color : 'transparent',
        border: primary ? 'none' : `1px solid ${color}33`,
        padding: '3px 8px',
        borderRadius: 4,
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );
}

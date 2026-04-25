import React from 'react';
import { Filter } from 'lucide-react';
import { useSessionsCard } from '#renderer/domains/sessions/runtime/useSessionsCard';
import type { SessionEntry } from '#renderer/global/types';
import { clamp } from '#renderer/global/service/utils';
import { StatusFilterMenu } from '#renderer/domains/sessions/ui/StatusFilterMenu';
import { SESSION_STATUS_OPTIONS } from '#renderer/domains/sessions/service/helpers';
import { SessionsEmptyState } from '#renderer/domains/sessions/ui/SessionsEmptyState';
import { SessionsList } from '#renderer/domains/sessions/ui/SessionsList';
import { Button } from '#renderer/global/ui/primitives/button';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';

interface SessionsCardProps {
  active?: boolean;
  onOpenSession?: (s: SessionEntry) => void;
}

export function SessionsView({
  active = true,
  onOpenSession: onOpenSessionProp,
}: SessionsCardProps = {}): React.ReactElement {
  const card = useSessionsCard({ active, onOpenSession: onOpenSessionProp });

  return (
    <div data-testid="sessions-view" className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-baseline justify-between">
        <h2 className="text-heading text-foreground">Sessions</h2>
      </div>

      {/* Filters -- category pills + status filter (hidden when no sessions) */}
      {card.categories.total > 0 && (
        <div className="flex items-center justify-between">
          {card.categories.sorted.length > 0 && (
            <div className="flex flex-wrap items-center gap-1">
              <Button
                variant={!card.filters.category ? 'default' : 'secondary'}
                size="xs"
                onClick={() => card.filters.handleCatClick('')}
                aria-label="Show all session categories"
              >
                all <span className="ml-0.5 opacity-60">{card.categories.total}</span>
              </Button>
              {card.categories.sorted.map((cat) => (
                <Button
                  key={cat}
                  variant={card.filters.category === cat ? 'default' : 'secondary'}
                  size="xs"
                  onClick={() => card.filters.handleCatClick(cat)}
                  aria-label={`Filter by ${cat}`}
                >
                  {cat}
                  <span className="ml-0.5 opacity-60">{card.categories.counts[cat]}</span>
                </Button>
              ))}
            </div>
          )}

          <StatusFilterMenu
            value={card.filters.status}
            options={SESSION_STATUS_OPTIONS}
            onChange={(v) => {
              card.filters.setStatus(v);
              card.pagination.setPage(1);
              card.filters.setMenuOpen(false);
            }}
            open={card.filters.menuOpen}
            onOpenChange={card.filters.setMenuOpen}
          >
            <Button variant="ghost" size="icon-xs" aria-label="Filter by status">
              <Filter size={14} />
              {card.filters.isFiltered && (
                <span className="text-[11px]">{card.filters.status}</span>
              )}
            </Button>
          </StatusFilterMenu>
        </div>
      )}

      {/* Content -- only show loading skeletons on first-ever fetch, not category switches */}
      {card.list.loading && !card.list.hasLoadedRef.current ? (
        <div className="space-y-2 py-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full" />
          ))}
        </div>
      ) : card.list.error ? (
        <div className="py-8 text-center text-body text-destructive">{card.list.error}</div>
      ) : card.list.sessions.length === 0 ? (
        <SessionsEmptyState />
      ) : (
        <SessionsList
          sessions={card.list.sessions}
          openSession={card.actions.openSession}
          focusedIndex={card.list.clampedFocusedIndex}
          sessionSeq={card.list.sessionSeqMap}
        />
      )}

      {/* Pagination */}
      {card.pagination.totalPages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              card.pagination.setPage((p) => clamp(p - 1, 1, card.pagination.totalPages))
            }
            disabled={card.pagination.page <= 1}
            aria-label="Previous page"
          >
            Prev
          </Button>
          <span className="text-code tabular-nums text-muted-foreground">
            {card.pagination.page} / {card.pagination.totalPages}
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              card.pagination.setPage((p) => clamp(p + 1, 1, card.pagination.totalPages))
            }
            disabled={card.pagination.page >= card.pagination.totalPages}
            aria-label="Next page"
          >
            Next
          </Button>
        </div>
      )}
    </div>
  );
}

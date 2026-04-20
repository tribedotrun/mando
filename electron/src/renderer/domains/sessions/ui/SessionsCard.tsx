import React from 'react';
import { Filter } from 'lucide-react';
import { useSessionsCard } from '#renderer/domains/sessions/runtime/useSessionsCard';
import type { SessionEntry } from '#renderer/global/types';
import { clamp } from '#renderer/global/service/utils';
import { StatusFilterMenu } from '#renderer/global/ui/StatusFilterMenu';
import { SESSION_STATUS_OPTIONS } from '#renderer/domains/sessions/service/helpers';
import { SessionsEmptyState } from '#renderer/domains/sessions/ui/SessionsHelpers';
import { SessionsList } from '#renderer/domains/sessions/ui/SessionsList';
import { Button } from '#renderer/global/ui/button';
import { Skeleton } from '#renderer/global/ui/skeleton';

interface SessionsCardProps {
  active?: boolean;
  onOpenSession?: (s: SessionEntry) => void;
}

export function SessionsCard({
  active = true,
  onOpenSession: onOpenSessionProp,
}: SessionsCardProps = {}): React.ReactElement {
  const {
    page,
    setPage,
    filterCategory,
    filterStatus,
    setFilterStatus,
    showFilterMenu,
    setShowFilterMenu,
    sessions,
    totalPages,
    categories,
    loading,
    error,
    hasLoadedRef,
    sessionSeqMap,
    clampedFocusedIndex,
    isFiltered,
    allTotal,
    sortedCats,
    handleCatClick,
    openSession,
  } = useSessionsCard({ active, onOpenSession: onOpenSessionProp });

  return (
    <div data-testid="sessions-card" className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-baseline justify-between">
        <h2 className="text-heading text-foreground">Sessions</h2>
      </div>

      {/* Filters -- category pills + status filter (hidden when no sessions) */}
      {allTotal > 0 && (
        <div className="flex items-center justify-between">
          {sortedCats.length > 0 && (
            <div className="flex flex-wrap items-center gap-1">
              <Button
                variant={!filterCategory ? 'default' : 'secondary'}
                size="xs"
                onClick={() => handleCatClick('')}
                aria-label="Show all session categories"
              >
                all <span className="ml-0.5 opacity-60">{allTotal}</span>
              </Button>
              {sortedCats.map((cat) => (
                <Button
                  key={cat}
                  variant={filterCategory === cat ? 'default' : 'secondary'}
                  size="xs"
                  onClick={() => handleCatClick(cat)}
                  aria-label={`Filter by ${cat}`}
                >
                  {cat}
                  <span className="ml-0.5 opacity-60">{categories[cat]}</span>
                </Button>
              ))}
            </div>
          )}

          <StatusFilterMenu
            value={filterStatus}
            options={SESSION_STATUS_OPTIONS}
            onChange={(v) => {
              setFilterStatus(v);
              setPage(1);
              setShowFilterMenu(false);
            }}
            open={showFilterMenu}
            onOpenChange={setShowFilterMenu}
          >
            <Button variant="ghost" size="icon-xs" aria-label="Filter by status">
              <Filter size={14} />
              {isFiltered && <span className="text-[11px]">{filterStatus}</span>}
            </Button>
          </StatusFilterMenu>
        </div>
      )}

      {/* Content -- only show loading skeletons on first-ever fetch, not category switches */}
      {loading && !hasLoadedRef.current ? (
        <div className="space-y-2 py-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full" />
          ))}
        </div>
      ) : error ? (
        <div className="py-8 text-center text-body text-destructive">{error}</div>
      ) : sessions.length === 0 ? (
        <SessionsEmptyState />
      ) : (
        <SessionsList
          sessions={sessions}
          openSession={openSession}
          focusedIndex={clampedFocusedIndex}
          sessionSeq={sessionSeqMap}
        />
      )}

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage((p) => clamp(p - 1, 1, totalPages))}
            disabled={page <= 1}
            aria-label="Previous page"
          >
            Prev
          </Button>
          <span className="text-code tabular-nums text-muted-foreground">
            {page} / {totalPages}
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage((p) => clamp(p + 1, 1, totalPages))}
            disabled={page >= totalPages}
            aria-label="Next page"
          >
            Next
          </Button>
        </div>
      )}
    </div>
  );
}

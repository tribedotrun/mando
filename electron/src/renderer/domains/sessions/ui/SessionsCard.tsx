import React, { useState, useCallback, useRef } from 'react';
import { Filter } from 'lucide-react';
import { useSessionsList } from '#renderer/domains/sessions/runtime/hooks';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import type { SessionEntry } from '#renderer/global/types';
import {
  clamp,
  copyToClipboard,
  getErrorMessage,
  indexNext,
  indexPrev,
} from '#renderer/global/service/utils';
import { StatusFilterMenu } from '#renderer/global/ui/StatusFilterMenu';
import {
  buildSessionSequence,
  sortCategories,
  buildResumeCmd,
  SESSION_STATUS_OPTIONS,
} from '#renderer/domains/sessions/service/helpers';
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
  const [page, setPage] = useState(1);
  const [filterCategory, setFilterCategory] = useState('');
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const [focusedIndex, setFocusedIndex] = useState(-1);

  const {
    data: sessionsData,
    isLoading: loading,
    error: sessionsError,
  } = useSessionsList(page, filterCategory || undefined, filterStatus);
  // Track whether we've ever loaded data -- suppress loading text on category switches
  const hasLoadedRef = useRef(false);
  if (sessionsData) hasLoadedRef.current = true;

  const sessions: SessionEntry[] = sessionsData?.sessions ?? [];
  const totalPages = sessionsData?.total_pages ?? 1;
  const categories: Record<string, number> = sessionsData?.categories ?? {};
  const error = sessionsError ? getErrorMessage(sessionsError, 'Failed to fetch sessions') : null;
  const sessionSeqMap = React.useMemo(() => buildSessionSequence(sessions), [sessions]);

  const [showFilterMenu, setShowFilterMenu] = useState(false);
  const isFiltered = filterStatus !== 'all';

  // Clamp focusedIndex inline -- derived from sessions.length
  const clampedFocusedIndex =
    sessions.length === 0
      ? -1
      : focusedIndex >= sessions.length
        ? sessions.length - 1
        : focusedIndex;

  const openSession = useCallback(
    (s: SessionEntry) => {
      onOpenSessionProp?.(s);
    },
    [onOpenSessionProp],
  );

  const handleCatClick = (cat: string) => {
    setFilterCategory(cat);
    setPage(1);
  };

  const resumeCmd = (s: SessionEntry) => buildResumeCmd(s.session_id, s.resume_cwd || s.cwd);

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      switch (key) {
        case 'j':
          e.preventDefault();
          setFocusedIndex((i) => indexNext(i, sessions.length - 1));
          break;
        case 'k':
          e.preventDefault();
          setFocusedIndex((i) => indexPrev(i));
          break;
        case 'Enter': {
          const s = sessions[clampedFocusedIndex];
          if (s) {
            e.preventDefault();
            openSession(s);
          }
          break;
        }
        case 'c': {
          const s = sessions[clampedFocusedIndex];
          if (s) {
            e.preventDefault();
            void copyToClipboard(resumeCmd(s), 'Command copied');
          }
          break;
        }
        case 'Escape':
          if (clampedFocusedIndex >= 0) {
            e.preventDefault();
            setFocusedIndex(-1);
          }
          break;
      }
    },
    [sessions, clampedFocusedIndex, openSession],
  );

  useViewKeyHandler(handleKey, active);

  const allTotal = Object.values(categories).reduce((a, b) => a + b, 0);
  const sortedCats = sortCategories(categories);

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

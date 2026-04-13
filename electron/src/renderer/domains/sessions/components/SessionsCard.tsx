import React, { useState, useCallback, useRef } from 'react';
import { Filter } from 'lucide-react';
import { useSessionsList } from '#renderer/hooks/queries';
import { useViewKeyHandler } from '#renderer/global/hooks/useKeyboardShortcuts';
import { useScrollIntoViewRef } from '#renderer/global/hooks/useScrollIntoViewRef';
import type { SessionEntry } from '#renderer/types';
import {
  clamp,
  copyToClipboard,
  getErrorMessage,
  indexNext,
  indexPrev,
  relativeTime,
} from '#renderer/utils';
import { StatusFilterMenu } from '#renderer/domains/captain';
import {
  buildSessionSequence,
  sessionTitle,
  sessionSubtitle,
  SessionsEmptyState,
  SessionDot,
} from '#renderer/domains/sessions/components/SessionsHelpers';
import { Button } from '#renderer/components/ui/button';
import { Skeleton } from '#renderer/components/ui/skeleton';
import { Table, TableBody, TableRow, TableCell } from '#renderer/components/ui/table';

const CATEGORY_ORDER = [
  'workers',
  'clarifier',
  'captain-review',
  'captain-ops',
  'rebase',
  'todo-parser',
  'scout',
  'system',
];

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

  const resumeCmd = (s: SessionEntry) => {
    const dir = s.resume_cwd || s.cwd;
    return dir ? `cd "${dir}" && claude -r ${s.session_id}` : `claude -r ${s.session_id}`;
  };

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
  const sortedCats = CATEGORY_ORDER.filter((c) => c in categories);
  for (const c of Object.keys(categories)) {
    if (!sortedCats.includes(c)) sortedCats.push(c);
  }

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

function SessionsList({
  sessions,
  openSession,
  focusedIndex = -1,
  sessionSeq,
}: {
  sessions: SessionEntry[];
  openSession: (s: SessionEntry) => void;
  focusedIndex?: number;
  sessionSeq: Map<string, number>;
}): React.ReactElement {
  const scrollRef = useScrollIntoViewRef();

  return (
    <Table>
      <TableBody>
        {sessions.map((s, idx) => {
          const seq = sessionSeq.get(s.session_id);
          const title = seq ? `${sessionTitle(s)} #${seq}` : sessionTitle(s);
          const subtitle = sessionSubtitle(s);

          return (
            <TableRow
              ref={idx === focusedIndex ? scrollRef : undefined}
              key={s.session_id}
              data-focused={idx === focusedIndex || undefined}
              className={`cursor-pointer ${idx === focusedIndex ? 'outline outline-2 outline-ring -outline-offset-2' : ''}`}
              onClick={() => openSession(s)}
            >
              {/* Status dot */}
              <TableCell className="w-5 pr-0">
                <SessionDot status={s.status} />
              </TableCell>

              {/* Title + subtitle */}
              <TableCell>
                <span className="flex min-w-0 items-baseline gap-2">
                  <span className="shrink-0 text-[13px] text-foreground">{title}</span>
                  {subtitle && (
                    <span className="truncate text-[11px] text-muted-foreground">{subtitle}</span>
                  )}
                </span>
              </TableCell>

              {/* Credential */}
              <TableCell className="text-right">
                {s.credential_label && (
                  <span className="text-[11px] text-muted-foreground">{s.credential_label}</span>
                )}
              </TableCell>

              {/* Time */}
              <TableCell className="text-right">
                <span className="tabular-nums text-[11px] text-muted-foreground">
                  {s.created_at ? relativeTime(s.created_at) : '\u2014'}
                </span>
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

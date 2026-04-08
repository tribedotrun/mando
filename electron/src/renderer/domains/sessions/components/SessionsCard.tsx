import React, { useState, useCallback, useRef } from 'react';
import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { Filter } from 'lucide-react';
import { fetchSessions, fetchTranscript } from '#renderer/domains/sessions/hooks/useApi';
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
import { SessionDetailPanel } from '#renderer/domains/sessions/components/SessionDetailPanel';
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
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '#renderer/components/ui/table';

const PER_PAGE = 50;
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

export function SessionsCard({ active = true }: { active?: boolean } = {}): React.ReactElement {
  const [page, setPage] = useState(1);
  const [filterCategory, setFilterCategory] = useState('');
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const [viewSession, setViewSession] = useState<SessionEntry | null>(null);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [transcriptLoading, setTranscriptLoading] = useState(false);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  const [focusedIndex, setFocusedIndex] = useState(-1);

  const {
    data: sessionsData,
    isLoading: loading,
    error: sessionsError,
  } = useQuery({
    queryKey: ['sessions', page, filterCategory],
    queryFn: () => fetchSessions(page, PER_PAGE, filterCategory || undefined),
    placeholderData: keepPreviousData,
  });
  // Track whether we've ever loaded data -- suppress loading text on category switches
  const hasLoadedRef = useRef(false);
  if (sessionsData) hasLoadedRef.current = true;

  const allSessions: SessionEntry[] = sessionsData?.sessions ?? [];
  const sessions =
    filterStatus === 'all' ? allSessions : allSessions.filter((s) => s.status === filterStatus);
  const totalPages = sessionsData?.total_pages ?? 1;
  const categories: Record<string, number> = sessionsData?.categories ?? {};
  const error = sessionsError ? getErrorMessage(sessionsError, 'Failed to fetch sessions') : null;
  const sessionSeqMap = React.useMemo(() => buildSessionSequence(allSessions), [allSessions]);

  const [showFilterMenu, setShowFilterMenu] = useState(false);
  const isFiltered = filterStatus !== 'all';

  // Clamp focusedIndex inline -- derived from sessions.length
  const clampedFocusedIndex =
    sessions.length === 0
      ? -1
      : focusedIndex >= sessions.length
        ? sessions.length - 1
        : focusedIndex;

  const openSession = useCallback((s: SessionEntry) => {
    setViewSession(s);
    setTranscript(null);
    setTranscriptError(null);
    setTranscriptLoading(true);
    void fetchTranscript(s.session_id)
      .then((res) => {
        setTranscript(res.markdown);
        setTranscriptLoading(false);
      })
      .catch((err) => {
        setTranscriptError(getErrorMessage(err, 'Failed to load transcript'));
        setTranscriptLoading(false);
      });
  }, []);

  const handleCatClick = (cat: string) => {
    setFilterCategory(cat);
    setPage(1);
  };

  const resumeCmd = (s: SessionEntry) => {
    const dir = s.resume_cwd || s.cwd;
    return dir ? `cd ${dir} && claude -r ${s.session_id}` : `claude -r ${s.session_id}`;
  };

  const hasDetailOpen = !!viewSession;

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      // Escape closes the detail panel when open
      if (hasDetailOpen) {
        if (key === 'Escape') {
          e.preventDefault();
          setViewSession(null);
        }
        return;
      }
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
    [hasDetailOpen, sessions, clampedFocusedIndex, openSession],
  );

  useViewKeyHandler(handleKey, active);

  const allTotal = Object.values(categories).reduce((a, b) => a + b, 0);
  const sortedCats = CATEGORY_ORDER.filter((c) => c in categories);
  for (const c of Object.keys(categories)) {
    if (!sortedCats.includes(c)) sortedCats.push(c);
  }

  // Full-page detail view when a session is selected
  if (viewSession) {
    return (
      <SessionDetailPanel
        session={viewSession}
        markdown={transcript}
        loading={transcriptLoading}
        error={transcriptError}
        onClose={() => setViewSession(null)}
        resumeCmd={resumeCmd(viewSession)}
        sequenceNum={sessionSeqMap.get(viewSession.session_id)}
      />
    );
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
              setShowFilterMenu(false);
            }}
            open={showFilterMenu}
            onOpenChange={setShowFilterMenu}
          >
            <Button
              variant={isFiltered ? 'secondary' : 'ghost'}
              size="icon-xs"
              aria-label="Filter by status"
            >
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
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          <TableHead className="w-5" />
          <TableHead>Session</TableHead>
          <TableHead className="w-[72px] text-right">Time</TableHead>
        </TableRow>
      </TableHeader>
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

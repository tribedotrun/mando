import React, { useState, useCallback, useRef } from 'react';
import { useQuery, keepPreviousData } from '@tanstack/react-query';
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

const PER_PAGE = 50;
const CATEGORY_ORDER = ['workers', 'clarifier', 'captain-review', 'captain-ops', 'scout', 'system'];

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
  // Track whether we've ever loaded data — suppress loading text on category switches
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
  const filterBtnRef = React.useRef<HTMLButtonElement>(null);
  const isFiltered = filterStatus !== 'all';

  // Clamp focusedIndex inline — derived from sessions.length
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
    fetchTranscript(s.session_id)
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
            copyToClipboard(resumeCmd(s), 'Command copied');
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
    <div data-testid="sessions-card" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Header */}
      <div className="flex items-baseline justify-between">
        <h2 className="text-heading" style={{ color: 'var(--color-text-1)' }}>
          Sessions
        </h2>
      </div>

      {/* Filters — category pills + status filter (hidden when no sessions) */}
      {allTotal > 0 && (
        <div className="flex items-center justify-between">
          {sortedCats.length > 0 && (
            <div className="flex items-center flex-wrap" style={{ gap: 4 }}>
              <button
                onClick={() => handleCatClick('')}
                aria-label="Show all session categories"
                className="text-[12px] font-medium transition-colors"
                style={{
                  background: !filterCategory ? 'var(--color-accent)' : 'var(--color-surface-2)',
                  color: !filterCategory ? 'var(--color-bg)' : 'var(--color-text-3)',
                  padding: '4px 10px',
                  borderRadius: 6,
                  border: 'none',
                  cursor: 'pointer',
                }}
              >
                all <span className="ml-0.5 opacity-60">{allTotal}</span>
              </button>
              {sortedCats.map((cat) => (
                <button
                  key={cat}
                  onClick={() => handleCatClick(cat)}
                  aria-label={`Filter by ${cat}`}
                  className="text-[12px] font-medium transition-colors"
                  style={{
                    background:
                      filterCategory === cat ? 'var(--color-accent)' : 'var(--color-surface-2)',
                    color: filterCategory === cat ? 'var(--color-bg)' : 'var(--color-text-3)',
                    padding: '4px 10px',
                    borderRadius: 6,
                    border: 'none',
                    cursor: 'pointer',
                  }}
                >
                  {cat}
                  <span className="ml-0.5 opacity-60">{categories[cat]}</span>
                </button>
              ))}
            </div>
          )}

          <div style={{ position: 'relative' }}>
            <button
              ref={filterBtnRef}
              onClick={() => setShowFilterMenu((v) => !v)}
              className="flex items-center transition-colors"
              style={{
                padding: '4px 6px',
                borderRadius: 6,
                border: 'none',
                cursor: 'pointer',
                background: isFiltered ? 'var(--color-surface-3)' : 'transparent',
                color: isFiltered ? 'var(--color-text-1)' : 'var(--color-text-4)',
                gap: 4,
              }}
              title="Filter by status"
              aria-label="Filter by status"
            >
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                <path
                  d="M2 4h12M4 8h8M6 12h4"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
              {isFiltered && <span style={{ fontSize: 11 }}>{filterStatus}</span>}
            </button>
            {showFilterMenu && (
              <StatusFilterMenu
                value={filterStatus}
                onChange={(v) => {
                  setFilterStatus(v);
                  setShowFilterMenu(false);
                }}
                onClose={() => setShowFilterMenu(false)}
              />
            )}
          </div>
        </div>
      )}

      {/* Content — only show loading text on first-ever fetch, not category switches */}
      {loading && !hasLoadedRef.current ? (
        <div className="py-8 text-center text-body" style={{ color: 'var(--color-text-3)' }}>
          Loading sessions...
        </div>
      ) : error ? (
        <div className="py-8 text-center text-body" style={{ color: 'var(--color-error)' }}>
          {error}
        </div>
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
          <button
            onClick={() => setPage((p) => clamp(p - 1, 1, totalPages))}
            disabled={page <= 1}
            aria-label="Previous page"
            className="rounded-md px-2.5 py-1 text-[13px] disabled:opacity-30"
            style={{ border: '1px solid var(--color-border)', color: 'var(--color-text-2)' }}
          >
            Prev
          </button>
          <span className="text-code tabular-nums" style={{ color: 'var(--color-text-3)' }}>
            {page} / {totalPages}
          </span>
          <button
            onClick={() => setPage((p) => clamp(p + 1, 1, totalPages))}
            disabled={page >= totalPages}
            aria-label="Next page"
            className="rounded-md px-2.5 py-1 text-[13px] disabled:opacity-30"
            style={{ border: '1px solid var(--color-border)', color: 'var(--color-text-2)' }}
          >
            Next
          </button>
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
    <div className="flex flex-col" style={{ gap: 1 }}>
      {/* Header */}
      <div
        className="flex items-center"
        style={{ padding: '6px 12px', borderBottom: '1px solid var(--color-border-subtle)' }}
      >
        <span className="text-label shrink-0" style={{ color: 'var(--color-text-4)', width: 20 }} />
        <span className="text-label flex-1" style={{ color: 'var(--color-text-4)' }}>
          Session
        </span>
        <span
          className="text-label"
          style={{ color: 'var(--color-text-4)', width: 72, textAlign: 'right' }}
        >
          Time
        </span>
      </div>

      {sessions.map((s, idx) => {
        const seq = sessionSeq.get(s.session_id);
        const title = seq ? `${sessionTitle(s)} #${seq}` : sessionTitle(s);
        const subtitle = sessionSubtitle(s);

        return (
          <div
            ref={idx === focusedIndex ? scrollRef : undefined}
            key={s.session_id}
            data-focused={idx === focusedIndex || undefined}
            role="button"
            aria-label={`Open session ${title}`}
            className="flex cursor-pointer items-center"
            style={{
              paddingBlock: 8,
              paddingInline: 12,
              gap: 12,
              background: 'var(--color-surface-1)',
              borderRadius: 'var(--radius-row)',
              outline: idx === focusedIndex ? '2px solid var(--color-accent)' : 'none',
              outlineOffset: -2,
            }}
            onClick={() => openSession(s)}
          >
            {/* Status dot */}
            <span className="shrink-0" style={{ width: 8 }}>
              <SessionDot status={s.status} />
            </span>

            {/* Title + subtitle */}
            <span className="min-w-0 flex-1 flex items-baseline gap-2" title={s.session_id}>
              <span className="shrink-0" style={{ fontSize: 13, color: 'var(--color-text-1)' }}>
                {title}
              </span>
              {subtitle && (
                <span className="truncate" style={{ fontSize: 11, color: 'var(--color-text-4)' }}>
                  {subtitle}
                </span>
              )}
            </span>

            {/* Time */}
            <span
              className="shrink-0 text-right tabular-nums"
              style={{ fontSize: 11, color: 'var(--color-text-3)', width: 72 }}
            >
              {s.created_at ? relativeTime(s.created_at) : '\u2014'}
            </span>
          </div>
        );
      })}
    </div>
  );
}

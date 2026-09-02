import React, { useMemo, useState } from 'react';
import { ArrowDown } from 'lucide-react';
import type { TaskProvider, TranscriptEvent } from '#renderer/global/types';
import {
  buildProviderTranscriptRenderRows,
  indexToolResults,
  resolveActiveBranch,
} from '#renderer/domains/sessions/service/transcriptEvents';
import { useFilteredTranscriptRows } from '#renderer/domains/sessions/runtime/useFilteredRows';
import { useStickyScroll } from '#renderer/domains/sessions/runtime/useStickyScroll';
import { ToolGroupBlock } from '#renderer/domains/sessions/ui/transcriptEvents/ToolGroupBlock';
import { TranscriptEventRow } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptEventRow';
import { TranscriptSearchBar } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptSearchBar';
import { TranscriptSessionProvider } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptSessionContext';

interface TranscriptMessageListProps {
  sessionId: string;
  events: readonly TranscriptEvent[];
  isRunning?: boolean;
  provider?: TaskProvider;
}

export function TranscriptMessageList({
  sessionId,
  events,
  isRunning,
  provider,
}: TranscriptMessageListProps): React.ReactElement {
  const active = useMemo(() => resolveActiveBranch(events, provider), [events, provider]);
  const toolResults = useMemo(() => indexToolResults(active), [active]);
  const renderRows = useMemo(
    () => buildProviderTranscriptRenderRows(active, provider),
    [active, provider],
  );
  const [searchQuery, setSearchQuery] = useState('');
  const { scrollRef, isAtBottom, scrollToBottom } = useStickyScroll(active.length);

  let initSeen = 0;
  const rows: React.ReactNode[] = renderRows.map((row) => {
    if (row.kind === 'tool_group') {
      return (
        <ToolGroupBlock
          key={row.id}
          id={row.group.id}
          tools={row.group.tools}
          results={toolResults}
          sessionId={sessionId}
        />
      );
    }
    if (row.event.kind === 'system_init') initSeen++;
    return (
      <TranscriptEventRow
        key={row.id}
        event={row.event}
        eventIndex={row.eventIndex}
        initBoundary={initSeen > 1}
        isSegmentResult={
          provider === 'claude' &&
          row.event.kind === 'result' &&
          active.slice(row.eventIndex + 1).some((event) => event.kind === 'system_init')
        }
        toolResults={toolResults}
        sessionId={sessionId}
      />
    );
  });

  const filtered = useFilteredTranscriptRows(rows, renderRows, searchQuery);

  return (
    <TranscriptSessionProvider sessionId={sessionId}>
      <div className="relative flex h-full min-h-0 flex-col">
        <TranscriptSearchBar value={searchQuery} onChange={setSearchQuery} />
        <div
          ref={scrollRef}
          data-testid="transcript-message-list"
          className="flex-1 overflow-y-auto px-4 py-3"
        >
          <div className="mx-auto flex max-w-[760px] flex-col gap-3">
            {filtered}
            {isRunning && (
              <div className="py-2 text-label italic text-muted-foreground" aria-live="polite">
                <span className="mr-2 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-accent align-middle" />
                running…
              </div>
            )}
          </div>
        </div>
        {!isAtBottom && (
          <button
            className="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-1 rounded-full border border-muted bg-background/95 px-3 py-1 text-label text-muted-foreground shadow hover:bg-muted"
            onClick={() => scrollToBottom()}
          >
            <ArrowDown size={12} /> latest
          </button>
        )}
      </div>
    </TranscriptSessionProvider>
  );
}

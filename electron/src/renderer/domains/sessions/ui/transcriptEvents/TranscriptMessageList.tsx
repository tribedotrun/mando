import React, { useMemo, useState } from 'react';
import { ArrowDown } from 'lucide-react';
import type { TranscriptEvent } from '#renderer/global/types';
import {
  indexToolResults,
  isCarrierUserEvent,
  resolveActiveBranch,
} from '#renderer/domains/sessions/service/transcriptEvents';
import { useFilteredTranscriptRows } from '#renderer/domains/sessions/runtime/useFilteredRows';
import { useStickyScroll } from '#renderer/domains/sessions/runtime/useStickyScroll';
import { AssistantMessage } from '#renderer/domains/sessions/ui/transcriptEvents/AssistantMessage';
import { SessionFooter } from '#renderer/domains/sessions/ui/transcriptEvents/SessionFooter';
import { SystemMessage } from '#renderer/domains/sessions/ui/transcriptEvents/SystemMessage';
import { UserMessage } from '#renderer/domains/sessions/ui/transcriptEvents/UserMessage';
import { TranscriptSearchBar } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptSearchBar';

interface TranscriptMessageListProps {
  events: readonly TranscriptEvent[];
  isRunning?: boolean;
}

export function TranscriptMessageList({
  events,
  isRunning,
}: TranscriptMessageListProps): React.ReactElement {
  const active = useMemo(() => resolveActiveBranch(events), [events]);
  const toolResults = useMemo(() => indexToolResults(active), [active]);
  const [searchQuery, setSearchQuery] = useState('');
  const { scrollRef, isAtBottom, scrollToBottom } = useStickyScroll(active.length);

  let initSeen = 0;
  const rows: React.ReactNode[] = active.map((event, index) => {
    if (isCarrierUserEvent(event)) return null;
    if (event.kind === 'system_init') {
      initSeen++;
      return (
        <SystemMessage
          key={index}
          event={{ kind: 'init', data: event.data, isBoundary: initSeen > 1 }}
        />
      );
    }
    if (event.kind === 'system_compact_boundary') {
      return <SystemMessage key={index} event={{ kind: 'compact', data: event.data }} />;
    }
    if (event.kind === 'system_status') {
      return <SystemMessage key={index} event={{ kind: 'status', data: event.data }} />;
    }
    if (event.kind === 'system_api_retry') {
      return <SystemMessage key={index} event={{ kind: 'retry', data: event.data }} />;
    }
    if (event.kind === 'system_local_command_output') {
      return <SystemMessage key={index} event={{ kind: 'local', data: event.data }} />;
    }
    if (event.kind === 'system_hook') {
      return <SystemMessage key={index} event={{ kind: 'hook', data: event.data }} />;
    }
    if (event.kind === 'system_rate_limit') {
      return <SystemMessage key={index} event={{ kind: 'ratelimit', data: event.data }} />;
    }
    if (event.kind === 'unknown') {
      return null;
    }
    if (event.kind === 'user') {
      return <UserMessage key={index} event={event.data} eventIndex={index} />;
    }
    if (event.kind === 'assistant') {
      return (
        <AssistantMessage
          key={index}
          event={event.data}
          eventIndex={index}
          toolResults={toolResults}
        />
      );
    }
    if (event.kind === 'tool_progress') {
      return null;
    }
    if (event.kind === 'result') {
      return <SessionFooter key={index} event={event.data} />;
    }
    return null;
  });

  const filtered = useFilteredTranscriptRows(rows, searchQuery);

  return (
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
  );
}

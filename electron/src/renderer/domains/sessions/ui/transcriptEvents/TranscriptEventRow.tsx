import React from 'react';
import type { TranscriptEvent, UserToolResultBlock } from '#renderer/global/types';
import { AssistantMessage } from '#renderer/domains/sessions/ui/transcriptEvents/AssistantMessage';
import { SessionFooter } from '#renderer/domains/sessions/ui/transcriptEvents/SessionFooter';
import { SystemMessage } from '#renderer/domains/sessions/ui/transcriptEvents/SystemMessage';
import { UserMessage } from '#renderer/domains/sessions/ui/transcriptEvents/UserMessage';

interface TranscriptEventRowProps {
  event: TranscriptEvent;
  eventIndex: number;
  initBoundary: boolean;
  toolResults: Map<string, UserToolResultBlock>;
}

export function TranscriptEventRow({
  event,
  eventIndex,
  initBoundary,
  toolResults,
}: TranscriptEventRowProps): React.ReactElement | null {
  if (event.kind === 'system_init') {
    return <SystemMessage event={{ kind: 'init', data: event.data, isBoundary: initBoundary }} />;
  }
  if (event.kind === 'system_compact_boundary') {
    return <SystemMessage event={{ kind: 'compact', data: event.data }} />;
  }
  if (event.kind === 'system_status') {
    return <SystemMessage event={{ kind: 'status', data: event.data }} />;
  }
  if (event.kind === 'system_api_retry') {
    return <SystemMessage event={{ kind: 'retry', data: event.data }} />;
  }
  if (event.kind === 'system_local_command_output') {
    return <SystemMessage event={{ kind: 'local', data: event.data }} />;
  }
  if (event.kind === 'system_hook') {
    return <SystemMessage event={{ kind: 'hook', data: event.data }} />;
  }
  if (event.kind === 'system_rate_limit') {
    return <SystemMessage event={{ kind: 'ratelimit', data: event.data }} />;
  }
  if (event.kind === 'system_thinking_tokens') {
    return <SystemMessage event={{ kind: 'thinking_tokens', data: event.data }} />;
  }
  if (event.kind === 'unknown') {
    return <SystemMessage event={{ kind: 'unknown', data: event.data }} />;
  }
  if (event.kind === 'user') {
    return <UserMessage event={event.data} eventIndex={eventIndex} />;
  }
  if (event.kind === 'assistant') {
    return (
      <AssistantMessage event={event.data} eventIndex={eventIndex} toolResults={toolResults} />
    );
  }
  if (event.kind === 'result') {
    return <SessionFooter event={event.data} />;
  }
  return null;
}

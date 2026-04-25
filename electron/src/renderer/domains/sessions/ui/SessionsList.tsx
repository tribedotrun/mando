import React from 'react';
import { useScrollIntoViewRef } from '#renderer/global/runtime/useScrollIntoViewRef';
import type { SessionEntry } from '#renderer/global/types';
import { relativeTime } from '#renderer/global/service/utils';
import { sessionTitle, sessionSubtitle } from '#renderer/domains/sessions/service/helpers';
import { SessionDot } from '#renderer/global/ui/SessionDot';
import { Table, TableBody, TableRow, TableCell } from '#renderer/global/ui/primitives/table';

export function SessionsList({
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
              <TableCell className="w-full max-w-0">
                <span className="flex min-w-0 items-baseline gap-2">
                  <span className="shrink-0 text-[13px] text-foreground" title={title}>
                    {title}
                  </span>
                  {subtitle && (
                    <span
                      className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground"
                      title={subtitle}
                    >
                      {subtitle}
                    </span>
                  )}
                </span>
              </TableCell>

              {/* Credential */}
              <TableCell className="text-right">
                {s.credential_label && (
                  <span
                    className="inline-block max-w-[120px] truncate text-[11px] text-muted-foreground"
                    title={s.credential_label}
                  >
                    {s.credential_label}
                  </span>
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

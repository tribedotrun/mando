import { useMemo, type ReactNode } from 'react';
import {
  transcriptEventSearchText,
  type TranscriptRenderRow,
} from '#renderer/domains/sessions/service/transcriptEvents';

export function useFilteredTranscriptRows(
  rows: ReactNode[],
  sourceRows: readonly TranscriptRenderRow[],
  query: string,
): ReactNode[] {
  return useMemo(() => {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) return rows;
    return rows.filter((row, index) => {
      if (row == null) return false;
      const source = sourceRows[index];
      return source
        ? source.searchEvents.some((event) =>
            transcriptEventSearchText(event).toLowerCase().includes(trimmed),
          )
        : false;
    });
  }, [rows, sourceRows, query]);
}

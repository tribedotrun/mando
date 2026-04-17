import type { DailyMerge } from '#renderer/global/types';

export const ACTIVITY_STRIP_DAYS = 56;

export function buildCountMap(merges: DailyMerge[]): Map<string, number> {
  const map = new Map<string, number>();
  for (const m of merges) map.set(m.date, m.count);
  return map;
}

export function lastNDays(n: number): string[] {
  const dates: string[] = [];
  const now = new Date();
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    dates.push(d.toISOString().slice(0, 10));
  }
  return dates;
}

export function computeThresholds(counts: number[]): [number, number, number] {
  const nonzero = counts.filter((c) => c > 0);
  if (nonzero.length === 0) return [1, 2, 3];
  const max = Math.max(...nonzero);
  if (max <= 3) return [1, 2, 3];
  const third = max / 3;
  return [Math.ceil(third), Math.ceil(third * 2), max];
}

/** Group dates into week columns (Mon-Sun), returning a 7-row x N-col grid. */
export function buildGrid(dates: string[]): (string | null)[][] {
  const rows: (string | null)[][] = Array.from({ length: 7 }, () => []);
  let col = 0;
  for (let i = 0; i < dates.length; i++) {
    const d = new Date(dates[i] + 'T00:00:00');
    const dow = (d.getDay() + 6) % 7; // Mon=0 .. Sun=6
    if (i > 0 && dow === 0) col++;
    while (rows[dow].length < col) rows[dow].push(null);
    rows[dow].push(dates[i]);
  }
  const maxCols = Math.max(...rows.map((r) => r.length));
  for (const row of rows) {
    while (row.length < maxCols) row.push(null);
  }
  return rows;
}

export interface ActivityStripData {
  grid: (string | null)[][];
  countMap: Map<string, number>;
  thresholds: [number, number, number];
  hasMerges: boolean;
}

export function cellStyle(
  count: number,
  thresholds: [number, number, number],
): { backgroundColor: string } {
  if (count === 0) return { backgroundColor: 'var(--muted)' };
  if (count <= thresholds[0])
    return { backgroundColor: 'color-mix(in oklch, var(--success) 22%, transparent)' };
  if (count <= thresholds[1])
    return { backgroundColor: 'color-mix(in oklch, var(--success) 48%, transparent)' };
  return { backgroundColor: 'color-mix(in oklch, var(--success) 78%, transparent)' };
}

export function formatActivityDate(dateStr: string): string {
  const d = new Date(dateStr + 'T00:00:00');
  return d.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
}

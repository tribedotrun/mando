import React from 'react';
import { useActivityStripData } from '#renderer/hooks/queries';
import { Tooltip, TooltipContent, TooltipTrigger } from '#renderer/components/ui/tooltip';

function cellStyle(count: number, thresholds: [number, number, number]): React.CSSProperties {
  if (count === 0) return { backgroundColor: 'var(--muted)' };
  if (count <= thresholds[0])
    return { backgroundColor: 'color-mix(in oklch, var(--success) 22%, transparent)' };
  if (count <= thresholds[1])
    return { backgroundColor: 'color-mix(in oklch, var(--success) 48%, transparent)' };
  return { backgroundColor: 'color-mix(in oklch, var(--success) 78%, transparent)' };
}

function formatDate(dateStr: string): string {
  const d = new Date(dateStr + 'T00:00:00');
  return d.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
}

export function ActivityStrip(): React.ReactElement | null {
  const { grid, countMap, thresholds, hasMerges } = useActivityStripData();

  if (!hasMerges) return null;

  return (
    <div data-testid="activity-strip" className="flex flex-col items-center gap-[2.5px] pt-3 pb-1">
      {grid.map((row, rowIdx) => (
        <div key={rowIdx} className="flex gap-[2.5px]">
          {row.map((date, colIdx) =>
            date ? (
              <Tooltip key={colIdx}>
                <TooltipTrigger asChild>
                  <div
                    className="h-[10px] w-[10px] rounded-[2px]"
                    style={cellStyle(countMap.get(date) ?? 0, thresholds)}
                  />
                </TooltipTrigger>
                <TooltipContent side="top" className="text-xs">
                  {formatDate(date)}: {countMap.get(date) ?? 0} merged
                </TooltipContent>
              </Tooltip>
            ) : (
              <div key={colIdx} className="h-[10px] w-[10px]" />
            ),
          )}
        </div>
      ))}
    </div>
  );
}

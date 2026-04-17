import React from 'react';
import { useActivityStripData } from '#renderer/domains/captain/runtime/hooks';
import { cellStyle, formatActivityDate } from '#renderer/domains/captain/service/activityStrip';
import { Tooltip, TooltipContent, TooltipTrigger } from '#renderer/global/ui/tooltip';

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
                    className="h-[10px] w-[10px] rounded-[4px]"
                    style={cellStyle(countMap.get(date) ?? 0, thresholds)}
                  />
                </TooltipTrigger>
                <TooltipContent side="top" className="text-xs">
                  {formatActivityDate(date)}: {countMap.get(date) ?? 0} merged
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

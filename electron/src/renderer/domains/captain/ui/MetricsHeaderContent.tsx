import React from 'react';
import { ChevronDown } from 'lucide-react';
import { ceilMinutes } from '#renderer/global/service/utils';

export function MetricsHeaderContent({
  activeCount,
  reviewingCount,
  mergingCount,
  staleCount,
  rateLimitSecs,
  expanded,
  onResume,
  resumePending,
}: {
  activeCount: number;
  reviewingCount: number;
  mergingCount: number;
  staleCount: number;
  rateLimitSecs: number;
  expanded: boolean;
  onResume?: () => void;
  resumePending?: boolean;
}) {
  return (
    <>
      <span className="text-label text-text-3">Workers</span>
      <span className={`text-[12px] leading-4 ${activeCount > 0 ? 'text-success' : 'text-text-4'}`}>
        {activeCount} active
      </span>
      {reviewingCount > 0 && (
        <span className="text-[12px] leading-4 text-review">{reviewingCount} reviewing</span>
      )}
      {mergingCount > 0 && (
        <span className="text-[12px] leading-4 text-success">{mergingCount} merging</span>
      )}
      {staleCount > 0 && (
        <span className="text-[12px] leading-4 text-stale">{staleCount} stale</span>
      )}
      {rateLimitSecs > 0 && (
        <span className="inline-flex items-center gap-1.5 text-[12px] leading-4 text-text-4">
          paused ~{ceilMinutes(rateLimitSecs)}m
          {onResume && (
            <button
              type="button"
              disabled={resumePending}
              className="rounded px-1 py-0.5 text-[11px] font-medium text-foreground hover:bg-accent disabled:opacity-50"
              onClick={(e) => {
                e.stopPropagation();
                onResume();
              }}
            >
              {resumePending ? 'Resuming...' : 'Resume'}
            </button>
          )}
        </span>
      )}
      <span className="flex-1" />
      {(activeCount > 0 || reviewingCount > 0 || mergingCount > 0 || staleCount > 0) && (
        <ChevronDown
          size={10}
          className={`transition-transform duration-150 ease-out ${expanded ? 'rotate-180' : ''}`}
        />
      )}
    </>
  );
}

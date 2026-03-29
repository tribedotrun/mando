import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchPrSummary } from '#renderer/api';
import { prLabel, prHref } from '#renderer/utils';
import { PrMarkdown } from '#renderer/components/PrMarkdown';

interface Props {
  itemId: number;
  pr: string;
  githubRepo: string;
}

export function PrSummaryPanel({ itemId, pr, githubRepo }: Props): React.ReactElement {
  const label = prLabel(pr);
  const href = prHref(pr, githubRepo);

  const {
    data,
    isLoading: loading,
    error: queryError,
  } = useQuery({
    queryKey: ['pr-summary', itemId],
    queryFn: () => fetchPrSummary(itemId),
  });

  const body = data?.summary ?? null;
  const error = queryError
    ? queryError instanceof Error
      ? queryError.message
      : 'Failed to fetch'
    : null;

  return (
    <div data-testid="pr-summary-panel">
      <div
        className="px-4 py-3 border-b"
        style={{
          background: 'color-mix(in srgb, var(--color-surface-2) 60%, transparent)',
          borderColor: 'color-mix(in srgb, var(--color-border) 30%, transparent)',
        }}
      >
        <div className="flex items-center gap-2 mb-2">
          <span
            className="font-mono text-[0.6rem] font-semibold uppercase tracking-wider"
            style={{ color: 'var(--color-accent)' }}
          >
            PR Summary
          </span>
          <a
            href={href}
            target="_blank"
            rel="noopener noreferrer"
            className="font-mono text-[0.55rem] hover:underline"
            style={{ color: 'color-mix(in srgb, var(--color-accent) 70%, transparent)' }}
          >
            {githubRepo}
            {label}
          </a>
        </div>

        {loading && (
          <div className="text-xs py-4" style={{ color: 'var(--color-text-3)' }}>
            Fetching PR description...
          </div>
        )}
        {error && (
          <div className="text-xs py-2" style={{ color: 'var(--color-error)' }}>
            {error}
          </div>
        )}
        {body === null && !loading && !error && (
          <div className="text-xs py-2" style={{ color: 'var(--color-text-3)' }}>
            PR has no description.
          </div>
        )}
        {body && (
          <div className="max-h-[400px] overflow-y-auto">
            <PrMarkdown text={body} />
          </div>
        )}
      </div>
    </div>
  );
}

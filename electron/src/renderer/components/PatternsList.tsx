import { useState } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useMemoryStore } from '#renderer/stores/memoryStore';

const STATUS_STYLES: Record<string, React.CSSProperties> = {
  pending: { backgroundColor: 'var(--color-accent-wash)', color: 'var(--color-stale)' },
  approved: { backgroundColor: 'var(--color-success-bg)', color: 'var(--color-success)' },
  dismissed: { backgroundColor: 'var(--color-surface-3)', color: 'var(--color-text-4)' },
};

export function PatternsList() {
  const {
    patterns,
    patternsLoading,
    patternsError,
    patternStatusFilter,
    setPatternStatusFilter,
    approvePattern,
    dismissPattern,
    fetchPatterns,
  } = useMemoryStore();

  const [actioningId, setActioningId] = useState<number | null>(null);

  useMountEffect(() => {
    fetchPatterns();
  });

  const handleAction = async (id: number, action: (id: number) => Promise<void>) => {
    setActioningId(id);
    await action(id);
    setActioningId(null);
  };

  if (patternsError) {
    return (
      <div className="py-2 text-xs" style={{ color: 'var(--color-error)' }}>
        {patternsError}
      </div>
    );
  }

  return (
    <div>
      {/* Status filter */}
      <div className="mb-3 flex gap-1">
        {['', 'pending', 'approved', 'dismissed'].map((s) => (
          <button
            key={s}
            onClick={() => setPatternStatusFilter(s)}
            className="rounded px-2 py-1 text-xs"
            style={
              patternStatusFilter === s
                ? {
                    backgroundColor: 'var(--color-surface-3)',
                    color: 'var(--color-text-1)',
                  }
                : { color: 'var(--color-text-4)' }
            }
          >
            {s || 'All'}
          </button>
        ))}
      </div>

      {patternsLoading ? (
        <div className="py-4 text-center text-xs" style={{ color: 'var(--color-text-4)' }}>
          Loading...
        </div>
      ) : patterns.length === 0 ? (
        <div className="py-4 text-center text-xs" style={{ color: 'var(--color-text-4)' }}>
          No patterns found. Run the distiller to analyze decision history.
        </div>
      ) : (
        <div className="space-y-2">
          {patterns.map((p) => (
            <div
              key={p.id}
              className="rounded border px-3 py-2"
              style={{
                borderColor: 'color-mix(in srgb, var(--color-border) 50%, transparent)',
                backgroundColor: 'color-mix(in srgb, var(--color-surface-2) 50%, transparent)',
              }}
            >
              <div className="mb-1 flex items-start justify-between gap-2">
                <span className="text-xs leading-snug" style={{ color: 'var(--color-text-2)' }}>
                  {p.pattern}
                </span>
                <div className="flex shrink-0 items-center gap-2">
                  <span
                    className="rounded px-1.5 py-0.5 text-[0.65rem] font-medium"
                    style={STATUS_STYLES[p.status] ?? STATUS_STYLES.pending}
                  >
                    {p.status}
                  </span>
                  <span className="text-[0.65rem]" style={{ color: 'var(--color-text-4)' }}>
                    {Math.round(p.confidence * 100)}%
                  </span>
                </div>
              </div>
              <div className="mb-1 text-xs" style={{ color: 'var(--color-text-4)' }}>
                {p.recommendation}
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[0.65rem]" style={{ color: 'var(--color-text-4)' }}>
                  n={p.sample_size}
                </span>
                {p.status === 'pending' && (
                  <div className="flex gap-1">
                    <button
                      onClick={() => handleAction(p.id, approvePattern)}
                      disabled={actioningId === p.id}
                      className="rounded px-2 py-0.5 text-[0.65rem] disabled:opacity-50"
                      style={{
                        backgroundColor: 'var(--color-success-bg)',
                        color: 'var(--color-success)',
                      }}
                    >
                      Approve
                    </button>
                    <button
                      onClick={() => handleAction(p.id, dismissPattern)}
                      disabled={actioningId === p.id}
                      className="rounded px-2 py-0.5 text-[0.65rem] disabled:opacity-50"
                      style={{
                        backgroundColor: 'var(--color-surface-3)',
                        color: 'var(--color-text-3)',
                      }}
                    >
                      Dismiss
                    </button>
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

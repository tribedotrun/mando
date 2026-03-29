import { useMemo } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useMemoryStore } from '#renderer/stores/memoryStore';

const OUTCOME_COLORS: Record<string, string> = {
  success: 'var(--color-success)',
  failure: 'var(--color-error)',
  terminal: 'var(--color-text-4)',
};

const ACTION_COLORS: Record<string, string> = {
  nudge: 'var(--color-stale)',
  restart: 'var(--color-error)',
  'awaiting-review': 'var(--color-accent)',
  'captain-reviewing': 'var(--color-accent)',
  'captain-merging': 'var(--color-accent)',
  'review-reopen': 'var(--color-stale)',
  escalated: 'var(--color-error)',
  errored: 'var(--color-error)',
};

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('en-US', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
  } catch {
    return iso;
  }
}

export function JournalTable() {
  const {
    decisions,
    totals,
    journalLoading,
    journalError,
    workerFilter,
    actionFilter,
    outcomeFilter,
    setWorkerFilter,
    setActionFilter,
    setOutcomeFilter,
    fetchJournal,
  } = useMemoryStore();

  useMountEffect(() => {
    fetchJournal();
  });

  // Derive unique values for filter dropdowns
  const workers = useMemo(() => [...new Set(decisions.map((d) => d.worker))].sort(), [decisions]);
  const actions = useMemo(() => [...new Set(decisions.map((d) => d.action))].sort(), [decisions]);

  // Client-side filter (action + outcome applied locally since API only filters by worker)
  const filtered = useMemo(() => {
    let items = decisions;
    if (actionFilter) items = items.filter((d) => d.action === actionFilter);
    if (outcomeFilter) {
      if (outcomeFilter === 'pending') {
        items = items.filter((d) => !d.outcome);
      } else {
        items = items.filter((d) => d.outcome === outcomeFilter);
      }
    }
    return items;
  }, [decisions, actionFilter, outcomeFilter]);

  if (journalError) {
    return (
      <div className="py-2 text-xs" style={{ color: 'var(--color-error)' }}>
        {journalError}
      </div>
    );
  }

  return (
    <div>
      {/* Stats bar */}
      <div className="mb-3 flex gap-4 text-xs">
        <span style={{ color: 'var(--color-text-3)' }}>
          {totals.total} <span style={{ color: 'var(--color-text-4)' }}>decisions</span>
        </span>
        <span style={{ color: 'var(--color-success)' }}>
          {totals.successes}{' '}
          <span style={{ color: 'var(--color-text-4)' }}>
            success
            {totals.total > 0 ? ` (${Math.round((totals.successes / totals.total) * 100)}%)` : ''}
          </span>
        </span>
        <span style={{ color: 'var(--color-error)' }}>
          {totals.failures}{' '}
          <span style={{ color: 'var(--color-text-4)' }}>
            fail
            {totals.total > 0 ? ` (${Math.round((totals.failures / totals.total) * 100)}%)` : ''}
          </span>
        </span>
        {totals.unresolved > 0 && (
          <span style={{ color: 'var(--color-stale)' }}>
            {totals.unresolved} <span style={{ color: 'var(--color-text-4)' }}>pending</span>
          </span>
        )}
      </div>

      {/* Filters */}
      <div className="mb-2 flex gap-2">
        <select
          value={workerFilter}
          onChange={(e) => setWorkerFilter(e.target.value)}
          className="rounded border px-2 py-1 text-xs"
          style={{
            borderColor: 'var(--color-border)',
            backgroundColor: 'var(--color-surface-2)',
            color: 'var(--color-text-2)',
          }}
        >
          <option value="">All workers</option>
          {workers.map((w) => (
            <option key={w} value={w}>
              {w}
            </option>
          ))}
        </select>
        <select
          value={actionFilter}
          onChange={(e) => setActionFilter(e.target.value)}
          className="rounded border px-2 py-1 text-xs"
          style={{
            borderColor: 'var(--color-border)',
            backgroundColor: 'var(--color-surface-2)',
            color: 'var(--color-text-2)',
          }}
        >
          <option value="">All actions</option>
          {actions.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
        <select
          value={outcomeFilter}
          onChange={(e) => setOutcomeFilter(e.target.value)}
          className="rounded border px-2 py-1 text-xs"
          style={{
            borderColor: 'var(--color-border)',
            backgroundColor: 'var(--color-surface-2)',
            color: 'var(--color-text-2)',
          }}
        >
          <option value="">All outcomes</option>
          <option value="success">success</option>
          <option value="failure">failure</option>
          <option value="terminal">terminal</option>
          <option value="pending">pending</option>
        </select>
      </div>

      {/* Table */}
      {journalLoading ? (
        <div className="py-4 text-center text-xs" style={{ color: 'var(--color-text-4)' }}>
          Loading...
        </div>
      ) : filtered.length === 0 ? (
        <div className="py-4 text-center text-xs" style={{ color: 'var(--color-text-4)' }}>
          No decisions recorded yet.
        </div>
      ) : (
        <div className="max-h-[400px] overflow-y-auto">
          <table className="w-full text-left font-mono text-[0.7rem]">
            <thead
              className="sticky top-0"
              style={{ backgroundColor: 'var(--color-surface-1)', color: 'var(--color-text-4)' }}
            >
              <tr>
                <th className="pb-1 pr-3 font-medium">Time</th>
                <th className="pb-1 pr-3 font-medium">Worker</th>
                <th className="pb-1 pr-3 font-medium">Action</th>
                <th className="pb-1 pr-3 font-medium">Outcome</th>
                <th className="pb-1 pr-3 font-medium">Source</th>
                <th className="pb-1 font-medium">Rule</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((d) => (
                <tr
                  key={d.id}
                  className="border-t"
                  style={{ borderColor: 'var(--color-border-subtle)' }}
                >
                  <td
                    className="whitespace-nowrap py-1 pr-3"
                    style={{ color: 'var(--color-text-4)' }}
                  >
                    {formatTime(d.created_at)}
                  </td>
                  <td
                    className="whitespace-nowrap py-1 pr-3"
                    style={{ color: 'var(--color-text-2)' }}
                  >
                    {d.worker}
                  </td>
                  <td
                    className="whitespace-nowrap py-1 pr-3"
                    style={{
                      color: ACTION_COLORS[d.action] ?? 'var(--color-text-2)',
                    }}
                  >
                    {d.action}
                  </td>
                  <td
                    className="whitespace-nowrap py-1 pr-3"
                    style={{
                      color: d.outcome
                        ? (OUTCOME_COLORS[d.outcome] ?? 'var(--color-text-3)')
                        : 'var(--color-text-4)',
                    }}
                  >
                    {d.outcome ?? 'pending'}
                  </td>
                  <td
                    className="whitespace-nowrap py-1 pr-3"
                    style={{ color: 'var(--color-text-4)' }}
                  >
                    {d.source}
                  </td>
                  <td
                    className="max-w-[300px] truncate py-1"
                    style={{ color: 'var(--color-text-3)' }}
                    title={d.rule}
                  >
                    {d.rule}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

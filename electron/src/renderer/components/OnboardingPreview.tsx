import React from 'react';

export type Feature = 'captain' | 'scout' | 'sessions';

export function PreviewPane({ feature }: { feature: Feature }): React.ReactElement {
  if (feature === 'captain') return <CaptainPreview />;
  if (feature === 'scout') return <ScoutPreview />;
  return <SessionsPreview />;
}

// ---- Shared styles ----

const panelStyle: React.CSSProperties = {
  width: '100%',
  borderRadius: 'var(--radius-panel)',
  background: 'var(--color-surface-2)',
  border: '1px solid var(--color-border-subtle)',
  overflow: 'hidden',
};

const headerStyle: React.CSSProperties = { padding: '12px 16px' };

const rowStyle: React.CSSProperties = {
  padding: '8px 12px',
  background: 'var(--color-surface-1)',
  borderRadius: 'var(--radius-row)',
  marginBottom: 1,
};

// ---- Captain preview ----

const MOCK_TASKS = [
  { title: 'Add dark mode toggle', project: 'mando', status: 'review' },
  { title: 'Fix auth middleware', project: 'mando', status: 'input needed' },
  { title: 'Update API docs', project: 'dash-ui', status: 'review' },
  { title: 'Refactor cache layer', project: 'payment-api', status: 'QUEUED' },
  { title: 'Add rate limiting', project: 'mando', status: 'merged' },
];

function statusColor(s: string): { bg: string; fg: string } {
  switch (s) {
    case 'review':
      return { bg: 'var(--color-review-bg)', fg: 'var(--color-review)' };
    case 'input needed':
      return { bg: 'var(--color-needs-human-bg)', fg: 'var(--color-needs-human)' };
    case 'merged':
      return { bg: 'var(--color-merged-bg)', fg: 'var(--color-text-3)' };
    default:
      return { bg: 'var(--color-surface-3)', fg: 'var(--color-text-3)' };
  }
}

function CaptainPreview(): React.ReactElement {
  return (
    <div style={panelStyle}>
      <div className="flex items-center justify-between" style={headerStyle}>
        <div className="flex items-center" style={{ gap: 12 }}>
          <span className="text-subheading" style={{ color: 'var(--color-text-1)' }}>
            Captain
          </span>
          <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
            All 5
          </span>
          <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
            Review 2
          </span>
          <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
            Input 1
          </span>
        </div>
        <div className="text-caption" style={{ color: 'var(--color-text-3)' }}>
          <span className="text-label">WORKERS</span>{' '}
          <span style={{ color: 'var(--color-success)' }}>3 active</span> <span>1 stale</span>
        </div>
      </div>
      <div style={{ padding: '0 8px 8px' }}>
        {MOCK_TASKS.map((task, i) => {
          const sc = statusColor(task.status);
          return (
            <div
              key={i}
              className="flex items-center justify-between"
              style={{ ...rowStyle, opacity: task.status === 'merged' ? 0.55 : 1 }}
            >
              <span className="text-body" style={{ color: 'var(--color-text-1)' }}>
                {task.title}
              </span>
              <div className="flex items-center" style={{ gap: 8 }}>
                <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                  {task.project}
                </span>
                <span
                  style={{
                    fontSize: 10,
                    fontWeight: 500,
                    padding: '2px 6px',
                    borderRadius: 3,
                    background: sc.bg,
                    color: sc.fg,
                    textTransform: task.status === 'QUEUED' ? 'uppercase' : undefined,
                  }}
                >
                  {task.status}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---- Scout preview ----

const MOCK_ITEMS = [
  { title: 'Claude 4.5 release notes', source: 'anthropic.com', age: '2h' },
  { title: 'Rust async patterns for CLI tools', source: 'blog.rust-lang.org', age: '5h' },
  { title: 'stellar/orbit-sdk — 3 new commits', source: 'GitHub', age: '1d' },
  { title: 'Building offline-first Electron apps', source: 'electron.dev', age: '2d' },
];

function ScoutPreview(): React.ReactElement {
  return (
    <div style={panelStyle}>
      <div className="flex items-center justify-between" style={headerStyle}>
        <span className="text-subheading" style={{ color: 'var(--color-text-1)' }}>
          Scout
        </span>
        <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
          4 new items
        </span>
      </div>
      <div style={{ padding: '0 8px 8px' }}>
        {MOCK_ITEMS.map((item, i) => (
          <div key={i} className="flex items-center justify-between" style={rowStyle}>
            <div className="flex flex-col" style={{ gap: 2 }}>
              <span className="text-body" style={{ color: 'var(--color-text-1)' }}>
                {item.title}
              </span>
              <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                {item.source}
              </span>
            </div>
            <span className="text-caption" style={{ color: 'var(--color-text-3)', flexShrink: 0 }}>
              {item.age}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---- Sessions preview ----

const MOCK_SESSIONS = [
  { task: 'Add dark mode toggle', model: 'opus', cost: '$0.42', dur: '3m 12s', status: 'done' },
  { task: 'Fix auth middleware', model: 'opus', cost: '$1.87', dur: '8m 45s', status: 'running' },
  { task: 'Update API docs', model: 'sonnet', cost: '$0.15', dur: '1m 03s', status: 'done' },
  { task: 'Refactor cache layer', model: 'opus', cost: '$0.91', dur: '5m 22s', status: 'done' },
];

function SessionsPreview(): React.ReactElement {
  return (
    <div style={panelStyle}>
      <div className="flex items-center justify-between" style={headerStyle}>
        <span className="text-subheading" style={{ color: 'var(--color-text-1)' }}>
          Sessions
        </span>
        <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
          Total $3.35
        </span>
      </div>
      <div style={{ padding: '0 8px 8px' }}>
        {MOCK_SESSIONS.map((s, i) => (
          <div key={i} className="flex items-center justify-between" style={rowStyle}>
            <div className="flex flex-col" style={{ gap: 2 }}>
              <span className="text-body" style={{ color: 'var(--color-text-1)' }}>
                {s.task}
              </span>
              <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                {s.model} · {s.dur}
              </span>
            </div>
            <div className="flex items-center" style={{ gap: 8 }}>
              <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                {s.cost}
              </span>
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 500,
                  padding: '2px 6px',
                  borderRadius: 3,
                  background:
                    s.status === 'running' ? 'var(--color-accent-bg)' : 'var(--color-surface-3)',
                  color: s.status === 'running' ? 'var(--color-accent)' : 'var(--color-text-3)',
                }}
              >
                {s.status}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

import React from 'react';
import { useQuery } from '@tanstack/react-query';

const cardStyle = {
  borderRadius: 'var(--radius-panel)',
  border: '1px solid var(--color-border)',
  background: 'var(--color-surface-1)',
  padding: '20px',
};

interface AppInfo {
  appVersion: string;
  stack: Array<{ name: string; version: string }>;
}

export function SettingsAbout(): React.ReactElement {
  const { data } = useQuery<AppInfo>({
    queryKey: ['settings', 'about', 'appInfo'],
    queryFn: async () => {
      if (!window.mandoAPI) return { appVersion: '', stack: [] };
      return window.mandoAPI.appInfo();
    },
  });

  const appVersion = data?.appVersion ?? '';
  const stack = data?.stack ?? [];

  return (
    <div data-testid="settings-about" className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
          About
        </h2>
        <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)' }}>
          Application details and links.
        </p>
      </div>

      <div className="space-y-6">
        <div className="text-center" style={{ ...cardStyle, padding: '24px' }}>
          <div
            className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl text-2xl font-bold"
            style={{
              background:
                'linear-gradient(135deg, var(--color-accent), var(--color-accent-pressed))',
              color: 'var(--color-bg)',
            }}
          >
            C
          </div>
          <h3 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
            Mando
          </h3>
          <p className="mt-1 text-sm" style={{ color: 'var(--color-text-2)' }}>
            Captain-driven development on macOS
          </p>
          <p className="mt-2 font-mono text-xs" style={{ color: 'var(--color-text-3)' }}>
            Version {appVersion || '\u2014'}
          </p>
        </div>

        <div style={cardStyle}>
          <h3 className="mb-4 text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
            Built With
          </h3>
          <div className="space-y-2">
            {stack.map((dep) => (
              <div key={dep.name} className="flex items-center justify-between">
                <span className="text-sm" style={{ color: 'var(--color-text-2)' }}>
                  {dep.name}
                </span>
                <span className="font-mono text-xs" style={{ color: 'var(--color-text-3)' }}>
                  {dep.version || '\u2014'}
                </span>
              </div>
            ))}
          </div>
        </div>

        <div style={cardStyle}>
          <h3 className="mb-4 text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
            Links
          </h3>
          <div className="space-y-2">
            <a
              href="https://github.com/tribedotrun/mando"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-sm"
              style={{ color: 'var(--color-accent)' }}
            >
              <svg className="h-4 w-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
              </svg>
              GitHub Repository
            </a>
            <a
              href="https://anthropic.com/claude-code"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-sm"
              style={{ color: 'var(--color-accent)' }}
            >
              <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
                />
              </svg>
              Claude Code
            </a>
          </div>
        </div>
      </div>
    </div>
  );
}

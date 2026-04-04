import React from 'react';
import { useQuery } from '@tanstack/react-query';

export function SettingsAbout(): React.ReactElement {
  const { data } = useQuery({
    queryKey: ['settings', 'about', 'appVersion'],
    queryFn: async () => {
      if (!window.mandoAPI) return '';
      const info = await window.mandoAPI.appInfo();
      return info.appVersion;
    },
  });

  const appVersion = data ?? '';

  return (
    <div data-testid="settings-about">
      <h2 className="text-heading" style={{ color: 'var(--color-text-1)', marginBottom: 24 }}>
        About
      </h2>

      <div
        className="flex items-center justify-between"
        style={{ padding: '10px 0', minHeight: 40 }}
      >
        <span className="text-body" style={{ color: 'var(--color-text-1)' }}>
          Mando
        </span>
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {appVersion || '\u2014'}
        </span>
      </div>

      <div style={{ padding: '10px 0' }}>
        <a
          href="https://github.com/tribedotrun/mando"
          target="_blank"
          rel="noopener noreferrer"
          className="text-body"
          style={{ color: 'var(--color-accent)' }}
        >
          GitHub
        </a>
      </div>
    </div>
  );
}

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
      <h2 className="mb-6 text-heading text-foreground">About</h2>

      <div className="flex min-h-[40px] items-center justify-between py-2.5">
        <span className="text-body text-foreground">Mando</span>
        <span className="text-code text-muted-foreground">{appVersion || '\u2014'}</span>
      </div>

      <div className="py-2.5">
        <a
          href="https://github.com/tribedotrun/mando"
          target="_blank"
          rel="noopener noreferrer"
          className="text-body text-muted-foreground hover:text-foreground"
        >
          GitHub
        </a>
      </div>
    </div>
  );
}

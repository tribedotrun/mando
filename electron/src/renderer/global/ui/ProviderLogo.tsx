import React from 'react';
import codexLogo from '#renderer/assets/codex-logo.png';
import claudeLogo from '#renderer/assets/claude-logo.png';
import type { TaskProvider } from '#renderer/global/types';
import { taskProviderLabel } from '#renderer/global/service/providerDisplay';

export function ProviderLogo({ provider }: { provider: TaskProvider }): React.ReactElement {
  const label = taskProviderLabel(provider);
  const logo = provider === 'codex' ? codexLogo : claudeLogo;

  return (
    <span
      data-testid={`workbench-row-provider-${provider}`}
      title={`Provider: ${label}`}
      className="flex size-4 shrink-0 items-center justify-center"
    >
      <img src={logo} alt={label} className="size-4 object-contain" />
    </span>
  );
}

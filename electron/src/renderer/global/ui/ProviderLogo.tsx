import React from 'react';
import codexLogo from '#renderer/assets/codex-logo.png';
import claudeLogo from '#renderer/assets/claude-logo.png';
import zaiLogo from '#renderer/assets/zai-logo.svg';
import type { TaskProvider } from '#renderer/global/types';
import {
  taskProviderLogoLabel,
  taskProviderLogoTitle,
} from '#renderer/global/service/providerDisplay';

export function ProviderLogo({
  provider,
  useGlmWorker,
}: {
  provider: TaskProvider;
  useGlmWorker: boolean;
}): React.ReactElement {
  const label = taskProviderLogoLabel(provider, useGlmWorker);
  const title = taskProviderLogoTitle(provider, useGlmWorker);
  const logo = provider === 'codex' ? codexLogo : provider === 'opencode' ? zaiLogo : claudeLogo;
  const showZaiWorker = provider !== 'opencode' && useGlmWorker;

  return (
    <span
      data-testid={`workbench-row-provider-${provider}`}
      data-glm-worker={useGlmWorker ? 'true' : 'false'}
      title={title}
      className="relative flex size-4 shrink-0 items-center justify-center"
    >
      <img src={logo} alt={label} className="size-4 object-contain" />
      {showZaiWorker && (
        <span className="absolute -right-0.5 -bottom-0.5 flex size-2.5 items-center justify-center overflow-hidden rounded-sm bg-background ring-1 ring-review">
          <img src={zaiLogo} alt="" className="size-2.5 object-contain" aria-hidden="true" />
        </span>
      )}
    </span>
  );
}

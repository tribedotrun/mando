import type { TaskProvider } from '#renderer/global/types';

export function taskProviderLabel(provider: TaskProvider): string {
  if (provider === 'codex') return 'Codex';
  if (provider === 'opencode') return 'OpenCode';
  return 'Claude Code';
}

export function taskProviderShortLabel(provider: TaskProvider): string {
  if (provider === 'codex') return 'Codex';
  if (provider === 'opencode') return 'OpenCode';
  return 'Claude';
}

export function taskProviderLogoLabel(provider: TaskProvider, useGlmWorker: boolean): string {
  if (provider === 'opencode') return 'OpenCode / Z.ai';
  const providerLabel = taskProviderLabel(provider);
  if (useGlmWorker) return `${providerLabel} with Z.ai GLM worker`;
  return providerLabel;
}

export function taskProviderLogoTitle(provider: TaskProvider, useGlmWorker: boolean): string {
  if (provider === 'opencode') return 'Provider: OpenCode / Z.ai';
  const providerLabel = taskProviderLabel(provider);
  if (useGlmWorker) return `Captain: ${providerLabel}. Worker: Z.ai GLM 5.2.`;
  return `Provider: ${providerLabel}`;
}

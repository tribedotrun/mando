import type { TaskProvider } from '#renderer/global/types';

export function taskProviderLabel(provider: TaskProvider): string {
  return provider === 'codex' ? 'Codex' : 'Claude Code';
}

export function taskProviderShortLabel(provider: TaskProvider): string {
  return provider === 'codex' ? 'Codex' : 'Claude';
}

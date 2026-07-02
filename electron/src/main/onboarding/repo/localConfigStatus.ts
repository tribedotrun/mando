import { parseJsonText } from '#result';

export function hasParsableLocalConfig(raw: string): boolean {
  const parsed = parseJsonText(raw, 'ipc:has-config local config');
  if (parsed.isErr()) return false;
  return typeof parsed.value === 'object' && parsed.value !== null && !Array.isArray(parsed.value);
}

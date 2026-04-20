import { apiErrorMessage, parseJsonTextWith } from '#result';
import { mandoConfigSchema } from './schemas.ts';

export function parseConfigJsonText(configJson: string, where: string) {
  return parseJsonTextWith(configJson, mandoConfigSchema, where);
}

export function requireConfigJsonText(configJson: string, where: string) {
  const parsed = parseConfigJsonText(configJson, where);
  if (parsed.isErr()) {
    // invariant: malformed config JSON must fail before local writes or daemon forwarding.
    throw new Error(apiErrorMessage(parsed.error), { cause: parsed.error });
  }
  return parsed.value;
}

export function requireValidConfigJsonText(configJson: string, where: string) {
  requireConfigJsonText(configJson, where);
  return configJson;
}

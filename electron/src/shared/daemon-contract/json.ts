import {
  type ApiError,
  type Result,
  apiErrorMessage,
  err,
  parseJsonText,
  parseJsonTextWith,
  parseWith,
} from '#result';
import type { MandoConfig } from './index.ts';
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

// Deepest nesting the config schema reaches, and therefore the most passes
// stripping unknown keys can need before the document validates.
const MAX_STRIP_PASSES = 8;

/**
 * Parse config JSON read straight off disk, dropping keys the current schema
 * no longer knows.
 *
 * `mandoConfigSchema` is generated `.strict()`, but a `config.json` written by
 * an older build legitimately carries keys that build has since retired, and
 * the Rust loader reads such a file happily (`Config` is not
 * `deny_unknown_fields`, so retired keys are dropped on read and disappear on
 * the next write). This mirrors that read behaviour so the local-file fallback
 * does not fail on a config the daemon itself accepts. Only unknown keys are
 * forgiven: a genuinely malformed config still errors.
 */
export function parseUpgradedConfigJsonText(
  configJson: string,
  where: string,
): Result<MandoConfig, ApiError> {
  const parsedJson = parseJsonText(configJson, where);
  if (parsedJson.isErr()) return err(parsedJson.error);

  let value = parsedJson.value;
  for (let pass = 0; pass <= MAX_STRIP_PASSES; pass += 1) {
    const parsed = parseWith(mandoConfigSchema, value, where);
    if (parsed.isOk()) return parsed;
    const unknownKeyPaths = unrecognizedKeysOf(parsed.error);
    if (unknownKeyPaths.length === 0) return parsed;
    value = withoutKeys(value, unknownKeyPaths);
  }
  return parseWith(mandoConfigSchema, value, where);
}

function unrecognizedKeysOf(error: ApiError): UnknownKeyPath[] {
  return error.code === 'parse' ? collectUnrecognizedKeys(error.issues) : [];
}

type UnknownKeyPath = { path: PropertyKey[]; keys: string[] };

function collectUnrecognizedKeys(issues: readonly unknown[]): UnknownKeyPath[] {
  const found: UnknownKeyPath[] = [];
  for (const issue of issues) {
    if (typeof issue !== 'object' || issue === null) continue;
    const record = issue as { code?: unknown; path?: unknown; keys?: unknown };
    if (record.code !== 'unrecognized_keys') continue;
    if (!Array.isArray(record.keys)) continue;
    found.push({
      path: Array.isArray(record.path) ? (record.path as PropertyKey[]) : [],
      keys: record.keys.filter((key): key is string => typeof key === 'string'),
    });
  }
  return found;
}

/** Structurally clone `root` without the named keys at each reported path. */
function withoutKeys(root: unknown, removals: UnknownKeyPath[]): unknown {
  return removals.reduce<unknown>(
    (acc, removal) => removeAt(acc, removal.path, removal.keys),
    root,
  );
}

function removeAt(node: unknown, path: PropertyKey[], keys: string[]): unknown {
  if (typeof node !== 'object' || node === null) return node;
  if (path.length === 0) {
    const next: Record<string, unknown> = { ...(node as Record<string, unknown>) };
    for (const key of keys) delete next[key];
    return next;
  }
  const [head, ...rest] = path;
  if (Array.isArray(node)) {
    const items = node as unknown[];
    const index = Number(head);
    if (!Number.isInteger(index) || index < 0 || index >= items.length) return node;
    return items.map((item, i) => (i === index ? removeAt(item, rest, keys) : item));
  }
  const key = String(head);
  const source = node as Record<string, unknown>;
  if (!(key in source)) return node;
  return { ...source, [key]: removeAt(source[key], rest, keys) };
}

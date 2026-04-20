export interface NormalizedClientLogContext {
  source: string | null;
  component: string | null;
  file: string | null;
  line: number | null;
  stack: string | null;
  sessionId: string | null;
  route: string | null;
  extra: string | null;
}

const CONTEXT_KEYS = new Set([
  'source',
  'component',
  'file',
  'line',
  'stack',
  'sessionId',
  'route',
  'extra',
]);

function asNullableString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function asNullableNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function stringifyExtra(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export function normalizeClientLogContext(context: unknown): NormalizedClientLogContext | null {
  if (context == null) return null;

  const record =
    typeof context === 'object' && !Array.isArray(context)
      ? (context as Record<string, unknown>)
      : { value: context };

  const extraFields: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(record)) {
    if (!CONTEXT_KEYS.has(key)) {
      extraFields[key] = value;
    }
  }

  const explicitExtra = stringifyExtra(record.extra);
  const trailingExtra = Object.keys(extraFields).length > 0 ? stringifyExtra(extraFields) : null;
  let extra: string | null = explicitExtra ?? trailingExtra;
  if (explicitExtra && trailingExtra && trailingExtra !== explicitExtra) {
    extra = `${explicitExtra}\n${trailingExtra}`;
  }

  return {
    source: asNullableString(record.source),
    component: asNullableString(record.component),
    file: asNullableString(record.file),
    line: asNullableNumber(record.line),
    stack: asNullableString(record.stack),
    sessionId: asNullableString(record.sessionId),
    route: asNullableString(record.route),
    extra,
  };
}

// no-restricted-imports uses the `ignore` package, which treats `#` as a
// comment. Patterns must escape it as `\#`. This helper hides that quirk so
// callers write the same alias they use everywhere else.

export function escapeAlias(pattern) {
  return pattern.replace(/^#/, '\\#');
}

export function group(patterns, message) {
  return { group: patterns.map(escapeAlias), message };
}

export const BAN_RELATIVE = {
  group: ['./*', '../*'],
  message: 'Use #renderer/ or #main/ aliases instead of relative imports.',
};

export const BAN_MAIN = group(['#main/*'], 'renderer cannot import from main.');
export const BAN_RENDERER = group(['#renderer/*'], 'main cannot import from renderer.');

export function restrictImports(...patterns) {
  return { 'no-restricted-imports': ['error', { patterns }] };
}

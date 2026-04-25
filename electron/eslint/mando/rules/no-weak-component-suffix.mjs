// Ban weak or ambiguous structural suffixes in component filenames. The
// codebase uses a single canonical term per role (see vocabulary table in
// CLAUDE.md). Multiple synonyms for "outer wrapper" or "inner primitives"
// make hierarchy unreadable from filenames.
//
// Banned filename suffixes:
//   - *Surface.tsx, *Shell.tsx, *Host.tsx, *Chrome.tsx (use *Frame)
//
// Banned filename suffixes when the file contains JSX:
//   - *Helpers.tsx, *Data.tsx (use *Parts for extracted primitives; a
//     "helpers" or "data" file should be .ts and not render JSX)
import path from 'node:path';

const BANNED_SUFFIXES = [
  { suffix: 'Surface', canonical: 'Frame' },
  { suffix: 'Shell', canonical: 'Frame' },
  { suffix: 'Host', canonical: 'Frame' },
  { suffix: 'Chrome', canonical: 'Frame' },
];

const JSX_ONLY_BANNED = [
  { suffix: 'Helpers', note: 'rename to *Parts, or move non-JSX helpers to a .ts sibling' },
  { suffix: 'Data', note: 'rename to *Parts, or split static data into a .ts sibling' },
];

function getStem(filename) {
  return path.basename(filename).replace(/\.(tsx|ts)$/, '');
}

function isRendererTsx(filename) {
  const norm = filename.replaceAll('\\', '/');
  return norm.includes('/src/renderer/') && norm.endsWith('.tsx');
}

function isExempt(filename) {
  const norm = filename.replaceAll('\\', '/');
  if (norm.includes('/renderer/global/ui/primitives/')) return true;
  if (norm.includes('/__tests__/') || norm.endsWith('.test.tsx')) return true;
  return false;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Ban weak structural suffixes in component filenames. Use canonical *Frame for outer wrappers and *Parts for extracted primitives.',
    },
    messages: {
      bannedSuffix:
        'Filename ends in banned suffix "*{{suffix}}.tsx". Use canonical "*{{canonical}}.tsx" instead (see vocabulary table in CLAUDE.md).',
      jsxInHelpersOrData:
        'File "{{stem}}.tsx" contains JSX but uses banned suffix "*{{suffix}}". {{note}}.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    if (!filename) return {};
    if (!isRendererTsx(filename)) return {};
    if (isExempt(filename)) return {};

    const stem = getStem(filename);

    // Hard filename bans: Surface/Shell/Host/Chrome fire regardless of JSX presence.
    for (const { suffix, canonical } of BANNED_SUFFIXES) {
      if (stem.length > suffix.length && stem.endsWith(suffix)) {
        return {
          Program(node) {
            context.report({
              node,
              messageId: 'bannedSuffix',
              data: { suffix, canonical },
            });
          },
        };
      }
    }

    // Conditional bans: Helpers/Data only when file contains JSX.
    for (const { suffix, note } of JSX_ONLY_BANNED) {
      if (stem.length > suffix.length && stem.endsWith(suffix)) {
        let containsJsx = false;
        return {
          'JSXElement, JSXFragment'() {
            containsJsx = true;
          },
          'Program:exit'(node) {
            if (containsJsx) {
              context.report({
                node,
                messageId: 'jsxInHelpersOrData',
                data: { stem, suffix, note },
              });
            }
          },
        };
      }
    }

    return {};
  },
};

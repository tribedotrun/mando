type Highlighter = {
  codeToHtml: (code: string, opts: { lang: string; theme: string }) => string;
};

let highlighterPromise: Promise<Highlighter> | null = null;

// invariant: singleton promise factory for the shiki library; callers receive the live Highlighter instance, not a Result-wrapped one
export function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    const p = import('shiki/bundle/full').then((mod) =>
      mod.createHighlighter({
        themes: ['github-dark-default'],
        langs: [
          'typescript',
          'javascript',
          'json',
          'bash',
          'python',
          'rust',
          'diff',
          'markdown',
          'yaml',
          'toml',
          'html',
          'css',
          'sql',
          'tsx',
          'jsx',
        ],
      }),
    );
    p.catch(() => {
      highlighterPromise = null;
    });
    highlighterPromise = p;
  }
  return highlighterPromise;
}

// invariant: syntax highlighter result is null on unsupported lang; callers treat null as plain-text fallback, errors are already caught inside
export async function highlight(code: string, lang: string): Promise<string | null> {
  try {
    const hl = await getHighlighter();
    return hl.codeToHtml(code, { lang, theme: 'github-dark-default' });
  } catch {
    return null;
  }
}

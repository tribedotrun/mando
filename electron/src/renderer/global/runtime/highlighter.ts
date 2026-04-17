import log from '#renderer/global/service/logger';

type Highlighter = {
  codeToHtml: (code: string, opts: { lang: string; theme: string }) => string;
};

// Singleton highlighter -- loaded once, reused everywhere
let highlighterPromise: Promise<Highlighter> | null = null;

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

export async function highlight(code: string, lang: string): Promise<string | null> {
  const hl = await getHighlighter();
  try {
    return hl.codeToHtml(code, { lang, theme: 'github-dark-default' });
  } catch (err) {
    log.warn('[highlight] codeToHtml failed for lang:', lang, err);
    return null;
  }
}

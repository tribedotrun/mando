type Highlighter = {
  codeToHtml: (code: string, opts: { lang: string; theme: string }) => string;
};

function createHighlighterState() {
  let highlighterPromise: Promise<Highlighter> | null = null;

  // invariant: singleton promise factory for shiki library interop; callers receive the live Highlighter instance
  async function createHighlighterInstance(): Promise<Highlighter> {
    const mod = await import('shiki/bundle/full');
    return mod.createHighlighter({
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
    });
  }

  async function clearFailedHighlighterLoad(promise: Promise<Highlighter>): Promise<void> {
    try {
      await promise;
    } catch {
      highlighterPromise = null;
    }
  }

  return {
    // invariant: singleton promise factory for the shiki library; callers receive the live Highlighter instance, not a Result-wrapped one
    getHighlighter(): Promise<Highlighter> {
      if (!highlighterPromise) {
        const promise = createHighlighterInstance();
        void clearFailedHighlighterLoad(promise);
        highlighterPromise = promise;
      }
      return highlighterPromise;
    },
  };
}

const highlighterState = createHighlighterState();

// invariant: syntax highlighter result is null on unsupported lang; callers treat null as plain-text fallback, errors are already caught inside
export async function highlight(code: string, lang: string): Promise<string | null> {
  try {
    const hl = await highlighterState.getHighlighter();
    return hl.codeToHtml(code, { lang, theme: 'github-dark-default' });
  } catch {
    return null;
  }
}

import type { IBufferLine, ILink, ILinkProvider, Terminal as XTerm } from '@xterm/xterm';
import log from '#renderer/logger';

const URL_RE = /\bhttps?:\/\/[^\s<>"')\]}]+/g;
const FILE_RE = /(?:^|[\s([{'"])((?:~\/|\.{1,2}\/|\/)[^\s"'`()[\]{}]+(?::\d+(?::\d+)?)?)/g;

/** Returns true when the cell at `col` is part of an OSC 8 hyperlink.
 *  Uses the internal CellData.extended.urlId property (non-zero = has link).
 *  xterm's own OscLinkProvider uses the same internal shape. */
function cellHasOscLink(line: IBufferLine, col: number): boolean {
  const cell = line.getCell(col);
  if (!cell) return false;
  const extended = (cell as { extended?: { urlId?: number } }).extended;
  return (extended?.urlId ?? 0) > 0;
}

interface PathTarget {
  filePath: string;
  line?: number;
  column?: number;
}

function lineText(terminal: XTerm, bufferLineNumber: number): string | null {
  return terminal.buffer.active.getLine(bufferLineNumber)?.translateToString(true) ?? null;
}

function createRange(bufferLineNumber: number, startIndex: number, text: string) {
  return {
    start: { x: startIndex + 1, y: bufferLineNumber + 1 },
    end: { x: startIndex + text.length, y: bufferLineNumber + 1 },
  };
}

function parsePathTarget(text: string): PathTarget {
  const match = /^(.*?)(?::(\d+))?(?::(\d+))?$/.exec(text);
  if (!match) return { filePath: text };
  return {
    filePath: match[1] || text,
    line: match[2] ? Number(match[2]) : undefined,
    column: match[3] ? Number(match[3]) : undefined,
  };
}

export function createUrlLinkProvider(opts: {
  terminal: XTerm;
  openUrl: (url: string) => Promise<void>;
}): ILinkProvider {
  return {
    provideLinks(bufferLineNumber, callback) {
      const text = lineText(opts.terminal, bufferLineNumber);
      callback(text ? findUrlLinks(bufferLineNumber, text, opts.openUrl) : undefined);
    },
  } as ILinkProvider;

  function findUrlLinks(
    bufferLineNumber: number,
    text: string,
    openUrlImpl: (url: string) => Promise<void>,
  ): ILink[] | undefined {
    const line = opts.terminal.buffer.active.getLine(bufferLineNumber);
    const links: ILink[] = [];
    for (const match of text.matchAll(URL_RE)) {
      const value = match[0];
      const index = match.index ?? -1;
      if (index < 0) continue;
      // Skip URLs that are already OSC 8 hyperlinks -- the built-in
      // OscLinkProvider handles those and firing both causes a double-open.
      if (line && cellHasOscLink(line, index)) continue;
      links.push({
        text: value,
        range: createRange(bufferLineNumber, index, value),
        decorations: { pointerCursor: true, underline: true },
        activate: () => {
          void openUrlImpl(value).catch((err) => log.warn('Failed to open terminal URL', err));
        },
      });
    }
    return links.length > 0 ? links : undefined;
  }
}

export function createFileLinkProvider(opts: {
  terminal: XTerm;
  cwd: string;
  resolvePath: (input: string, cwd: string) => Promise<string | null>;
  openPath: (path: string) => Promise<void>;
}): ILinkProvider {
  const cache = new Map<string, string | null>();

  const resolveCached = async (rawPath: string): Promise<string | null> => {
    const key = `${opts.cwd}::${rawPath}`;
    if (cache.has(key)) return cache.get(key) ?? null;
    try {
      const resolved = await opts.resolvePath(rawPath, opts.cwd);
      cache.set(key, resolved);
      return resolved;
    } catch (err) {
      log.warn('Failed to resolve terminal file path', err);
      cache.delete(key);
      return null;
    }
  };

  return {
    provideLinks(bufferLineNumber, callback) {
      const text = lineText(opts.terminal, bufferLineNumber);
      if (!text) {
        callback(undefined);
        return;
      }

      const matches = Array.from(text.matchAll(FILE_RE));
      if (matches.length === 0) {
        callback(undefined);
        return;
      }

      void Promise.all(
        matches.map(async (match) => {
          const matchedText = match[1];
          const startIndex = (match.index ?? -1) + match[0].indexOf(matchedText);
          if (!matchedText || startIndex < 0) return null;
          const parsed = parsePathTarget(matchedText);
          const resolved = await resolveCached(parsed.filePath);
          if (!resolved) return null;
          return {
            text: matchedText,
            range: createRange(bufferLineNumber, startIndex, matchedText),
            decorations: { pointerCursor: true, underline: true },
            activate: () => {
              void opts
                .openPath(resolved)
                .catch((err) => log.warn('Failed to open terminal file path', err));
            },
          } satisfies ILink;
        }),
      )
        .then((links) => {
          const validLinks = links.filter(Boolean) as ILink[];
          callback(validLinks.length > 0 ? validLinks : undefined);
        })
        .catch((err) => {
          log.warn('Failed to resolve terminal file links', err);
          callback(undefined);
        });
    },
  };
}

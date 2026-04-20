import type { IDisposable, Terminal as XTerm } from '@xterm/xterm';
import { openExternalUrl } from '#renderer/global/providers/native/shell';
import {
  resolveLocalPath,
  openLocalPath,
} from '#renderer/domains/captain/terminal/providers/terminalNative';
import {
  createFileLinkProvider,
  createUrlLinkProvider,
} from '#renderer/domains/captain/terminal/service/terminalLinks';
import log from '#renderer/global/service/logger';

export function installTerminalLinkProviders(term: XTerm, cwd: string): IDisposable[] {
  let lastOpenedUrl = '';
  let lastOpenedAt = 0;
  const openUrl = (url: string) => {
    const now = Date.now();
    if (url === lastOpenedUrl && now - lastOpenedAt < 500) return Promise.resolve();
    lastOpenedUrl = url;
    lastOpenedAt = now;
    return openExternalUrl(url);
  };
  const resolvePath = (input: string, pathCwd: string) => resolveLocalPath(input, pathCwd);
  const openPath = (filePath: string) => openLocalPath(filePath);

  term.options.linkHandler = {
    activate: (_event, url) => {
      void openUrl(url).catch((err) => log.warn('OSC 8 link open failed', err));
    },
  };

  return [
    term.registerLinkProvider(createUrlLinkProvider({ terminal: term, openUrl })),
    term.registerLinkProvider(
      createFileLinkProvider({
        terminal: term,
        cwd,
        resolvePath,
        openPath,
      }),
    ),
  ];
}

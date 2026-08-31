import fs from 'fs';
import path from 'path';
import type { ResultOf } from '#shared/ipc-contract';
import {
  EVIDENCE_DECK_ASSET_MIME_TYPES,
  EVIDENCE_DECK_DIRECTORY,
  EVIDENCE_DECK_FILENAME,
} from '#main/shell/config/evidenceDeck';

type EvidenceDeckDocument = NonNullable<ResultOf<'shell:read-evidence-deck'>>;

function errorCode(error: unknown): string | undefined {
  if (typeof error !== 'object' || error === null || !('code' in error)) return undefined;
  const code = (error as { code?: unknown }).code;
  return typeof code === 'string' ? code : undefined;
}

// invariant: filesystem failures must reject so the instrumented IPC boundary can report them
async function collectAssets(
  directory: string,
  relativeDirectory = '',
): Promise<EvidenceDeckDocument['assets']> {
  const entries = await fs.promises.readdir(directory, { withFileTypes: true });
  const assets: EvidenceDeckDocument['assets'] = [];

  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const absolutePath = path.join(directory, entry.name);
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    if (entry.isDirectory()) {
      assets.push(...(await collectAssets(absolutePath, relativePath)));
      continue;
    }
    if (!entry.isFile() || relativePath === EVIDENCE_DECK_FILENAME) continue;

    const mime = EVIDENCE_DECK_ASSET_MIME_TYPES[path.extname(entry.name).toLowerCase()];
    if (!mime) continue;
    const bytes = await fs.promises.readFile(absolutePath);
    assets.push({ path: relativePath, mime, base64: bytes.toString('base64') });
  }

  return assets;
}

function evidenceDeckPath(worktree: string): string | null {
  if (!path.isAbsolute(worktree)) return null;
  return path.join(worktree, EVIDENCE_DECK_DIRECTORY, EVIDENCE_DECK_FILENAME);
}

// invariant: IPC passthrough returns false for absence and rejects unexpected filesystem failures
export async function evidenceDeckExists(worktree: string): Promise<boolean> {
  const deckPath = evidenceDeckPath(worktree);
  if (!deckPath) return false;
  try {
    return (await fs.promises.stat(deckPath)).isFile();
  } catch (error: unknown) {
    if (errorCode(error) === 'ENOENT') return false;
    // invariant: non-missing filesystem failures must reach IPC instrumentation
    throw error;
  }
}

// invariant: IPC passthrough returns null for absence and rejects unexpected filesystem failures
export async function readEvidenceDeck(worktree: string): Promise<EvidenceDeckDocument | null> {
  const deckPath = evidenceDeckPath(worktree);
  if (!deckPath) return null;
  const evidenceDirectory = path.dirname(deckPath);
  let deckStat: fs.Stats;
  try {
    deckStat = await fs.promises.stat(deckPath);
  } catch (error: unknown) {
    if (errorCode(error) === 'ENOENT') return null;
    // invariant: non-missing filesystem failures must reach IPC instrumentation
    throw error;
  }
  if (!deckStat.isFile()) return null;

  const html = await fs.promises.readFile(deckPath, 'utf8');
  const assets = await collectAssets(evidenceDirectory);
  return { html, modifiedAtMs: deckStat.mtimeMs, assets };
}

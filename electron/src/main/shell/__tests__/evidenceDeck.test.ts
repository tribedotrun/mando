import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, it } from 'node:test';
import { evidenceDeckExists, readEvidenceDeck } from '#main/shell/runtime/evidenceDeck.ts';

const temporaryWorktrees: string[] = [];

async function createWorktree(): Promise<string> {
  const worktree = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'mando-evidence-deck-'));
  temporaryWorktrees.push(worktree);
  await fs.promises.mkdir(path.join(worktree, '.ai', 'evidence'), { recursive: true });
  return worktree;
}

afterEach(async () => {
  await Promise.all(
    temporaryWorktrees
      .splice(0)
      .map((worktree) => fs.promises.rm(worktree, { recursive: true, force: true })),
  );
});

describe('evidence deck loader', () => {
  it('loads only the latest canonical deck and its supported assets', async () => {
    const worktree = await createWorktree();
    const evidenceDirectory = path.join(worktree, '.ai', 'evidence');
    await fs.promises.writeFile(path.join(evidenceDirectory, 'deck-old.html'), 'old deck');
    await fs.promises.writeFile(path.join(evidenceDirectory, 'deck.html'), 'latest deck');
    await fs.promises.writeFile(path.join(evidenceDirectory, 'proof.png'), 'proof');
    await fs.promises.writeFile(path.join(evidenceDirectory, 'notes.txt'), 'not embedded');

    assert.equal(await evidenceDeckExists(worktree), true);
    const deck = await readEvidenceDeck(worktree);

    assert.equal(deck?.html, 'latest deck');
    assert.deepEqual(
      deck?.assets.map((asset) => asset.path),
      ['proof.png'],
    );
  });

  it('reports unavailable for missing or non-absolute worktrees', async () => {
    const worktree = await createWorktree();

    assert.equal(await evidenceDeckExists(worktree), false);
    assert.equal(await evidenceDeckExists('relative/worktree'), false);
    assert.equal(await readEvidenceDeck(worktree), null);
  });
});

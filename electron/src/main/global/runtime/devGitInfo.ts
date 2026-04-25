import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import log from '#main/global/providers/logger';
import { mustParseNonEmptyText, mustParseTrimmedText } from '#main/global/service/boundaryText';

export interface DevGitInfo {
  branch: string;
  commit: string;
  worktree: string | null;
  slot: string | null;
}

export function getDevGitInfo(): DevGitInfo {
  try {
    const branch = mustParseNonEmptyText(
      execSync('git rev-parse --abbrev-ref HEAD', { encoding: 'utf-8' }),
      'command:git-branch',
    );
    const commit = mustParseNonEmptyText(
      execSync('git rev-parse --short HEAD', { encoding: 'utf-8' }),
      'command:git-commit',
    );
    const toplevel = mustParseNonEmptyText(
      execSync('git rev-parse --show-toplevel', { encoding: 'utf-8' }),
      'command:git-toplevel',
    );
    const dirName = path.basename(toplevel);
    const parentName = path.basename(path.dirname(toplevel));
    const worktree = parentName === 'worktrees' ? dirName : null;
    const slotFile = path.join(toplevel, '.dev', 'slot');
    const slot = fs.existsSync(slotFile)
      ? mustParseTrimmedText(fs.readFileSync(slotFile, 'utf-8'), `file:${slotFile}`)
      : null;
    return { branch, commit, worktree, slot };
  } catch (error: unknown) {
    log.debug('[get-dev-git-info] git info failed:', error);
    return { branch: 'unknown', commit: 'unknown', worktree: null, slot: null };
  }
}

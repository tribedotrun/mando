import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import log from '#main/global/providers/logger';

export interface DevGitInfo {
  branch: string;
  commit: string;
  worktree: string | null;
  slot: string | null;
}

export function getDevGitInfo(): DevGitInfo {
  try {
    const branch = execSync('git rev-parse --abbrev-ref HEAD', { encoding: 'utf-8' }).trim();
    const commit = execSync('git rev-parse --short HEAD', { encoding: 'utf-8' }).trim();
    const toplevel = execSync('git rev-parse --show-toplevel', { encoding: 'utf-8' }).trim();
    const dirName = path.basename(toplevel);
    const parentName = path.basename(path.dirname(toplevel));
    const worktree = parentName === 'worktrees' ? dirName : null;
    const slotFile = path.join(toplevel, '.dev', 'slot');
    const slot = fs.existsSync(slotFile) ? fs.readFileSync(slotFile, 'utf-8').trim() : null;
    return { branch, commit, worktree, slot };
  } catch (error: unknown) {
    log.debug('[get-dev-git-info] git info failed:', error);
    return { branch: 'unknown', commit: 'unknown', worktree: null, slot: null };
  }
}

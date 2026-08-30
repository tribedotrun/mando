/**
 * Location of the worker-produced evidence deck inside a task worktree.
 * The path is a convention shared with the worker prompts and the captain
 * review gates; the worker writes the deck and its media there.
 */
export function evidenceDeckPath(worktree: string): string {
  return `${worktree.replace(/\/+$/, '')}/.ai/evidence/deck.html`;
}

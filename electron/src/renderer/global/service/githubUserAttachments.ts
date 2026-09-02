const GITHUB_USER_ATTACHMENT =
  /^https:\/\/github\.com\/user-attachments\/assets\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(?:[?#].*)?$/i;

export function githubUserAttachmentId(url: string | undefined): string | null {
  if (!url) return null;
  return GITHUB_USER_ATTACHMENT.exec(url)?.[1]?.toLowerCase() ?? null;
}

export function isBareGitHubUserAttachment(href: string | undefined, label: string): boolean {
  return githubUserAttachmentId(href) !== null && href === label;
}

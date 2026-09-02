import { buildUrl, staticRoutePath } from '#renderer/global/providers/http';
import { githubUserAttachmentId } from '#renderer/global/service/githubUserAttachments';

export function resolveGitHubUserAttachmentUrl(source: string | undefined): string | undefined {
  const assetId = githubUserAttachmentId(source);
  if (!assetId) return source;
  return buildUrl(staticRoutePath('getGithubAttachmentsById', { params: { id: assetId } }));
}

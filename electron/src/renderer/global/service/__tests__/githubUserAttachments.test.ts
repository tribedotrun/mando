import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { githubUserAttachmentId, isBareGitHubUserAttachment } from '../githubUserAttachments.ts';

const ASSET_URL = 'https://github.com/user-attachments/assets/196ce199-c4c7-4761-8779-a77e02234ae5';

describe('GitHub user attachments', () => {
  it('extracts the exact GitHub attachment id', () => {
    assert.equal(githubUserAttachmentId(ASSET_URL), '196ce199-c4c7-4761-8779-a77e02234ae5');
  });

  it('rejects arbitrary hosts, paths, and non-UUID ids', () => {
    assert.equal(githubUserAttachmentId('https://example.com/user-attachments/assets/x'), null);
    assert.equal(githubUserAttachmentId('https://github.com/owner/repo/assets/196ce199'), null);
    assert.equal(
      githubUserAttachmentId('https://github.com/user-attachments/assets/not-a-uuid'),
      null,
    );
  });

  it('identifies only a bare attachment autolink as embedded media', () => {
    assert.equal(isBareGitHubUserAttachment(ASSET_URL, ASSET_URL), true);
    assert.equal(isBareGitHubUserAttachment(ASSET_URL, 'watch the recording'), false);
  });
});

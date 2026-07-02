import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { compareSemver } from '../lifecycle.ts';

describe('compareSemver', () => {
  it('orders numeric prerelease identifiers numerically', () => {
    assert.equal(Math.sign(compareSemver('0.1.20-beta.12', '0.1.20-beta.3')), 1);
    assert.equal(Math.sign(compareSemver('0.1.20-beta.3', '0.1.20-beta.12')), -1);
  });

  it('treats identical prerelease versions as equal', () => {
    assert.equal(compareSemver('0.1.20-beta.12', '0.1.20-beta.12'), 0);
  });

  it('keeps stable releases newer than prereleases for the same base version', () => {
    assert.equal(Math.sign(compareSemver('0.1.20', '0.1.20-beta.12')), 1);
    assert.equal(Math.sign(compareSemver('0.1.20-beta.12', '0.1.20')), -1);
  });

  it('orders prereleases by base version before prerelease suffix', () => {
    assert.equal(Math.sign(compareSemver('0.1.21-beta.0', '0.1.20')), 1);
    assert.equal(Math.sign(compareSemver('0.1.19', '0.1.20-beta.0')), -1);
  });
});

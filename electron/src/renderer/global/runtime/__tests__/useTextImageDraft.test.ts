import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  applyQuotaFallback,
  decodeBase64ToBytes,
  draftSchema,
  encodeBytesToBase64,
  estimateSerializedLength,
  hasContent,
  isTextOnlyOversize,
  shouldExpire,
  TTL_MS,
} from '../useTextImageDraft.helpers.ts';

describe('useTextImageDraft.helpers', () => {
  describe('shouldExpire', () => {
    it('returns false just under the TTL boundary', () => {
      const saved = 1000;
      assert.equal(shouldExpire(saved, saved + TTL_MS - 1), false);
      assert.equal(shouldExpire(saved, saved + TTL_MS), false);
    });

    it('returns true just past the TTL boundary', () => {
      const saved = 1000;
      assert.equal(shouldExpire(saved, saved + TTL_MS + 1), true);
    });

    it('accepts an explicit ttl override', () => {
      assert.equal(shouldExpire(0, 10, 5), true);
      assert.equal(shouldExpire(0, 4, 5), false);
    });
  });

  describe('draftSchema', () => {
    it('parses a text-only draft with a null image', () => {
      const parsed = draftSchema.parse({ text: 'hi', image: null, savedAt: 42 });
      assert.equal(parsed.text, 'hi');
      assert.equal(parsed.image, null);
      assert.equal(parsed.savedAt, 42);
    });

    it('parses a draft with an image payload', () => {
      const parsed = draftSchema.parse({
        text: '',
        image: { base64: 'aGVsbG8=', name: 'a.png', mime: 'image/png' },
        savedAt: 1,
      });
      assert.equal(parsed.image?.name, 'a.png');
      assert.equal(parsed.image?.mime, 'image/png');
    });

    it('rejects a draft missing savedAt (old text-only shape)', () => {
      assert.throws(() => draftSchema.parse({ text: 'hi', image: null }));
    });

    it('rejects a non-object value', () => {
      assert.throws(() => draftSchema.parse('hi'));
    });

    it('rejects a malformed image block', () => {
      assert.throws(() =>
        draftSchema.parse({
          text: 'x',
          image: { base64: 'a', name: 'b.png' }, // missing mime
          savedAt: 1,
        }),
      );
    });
  });

  describe('applyQuotaFallback', () => {
    it('returns the draft unchanged when under the limit', () => {
      const draft = {
        text: 'hi',
        image: { base64: 'aGVsbG8=', name: 'a.png', mime: 'image/png' },
        savedAt: 1,
      };
      const result = applyQuotaFallback(draft, 10_000);
      assert.equal(result.dropped, false);
      assert.strictEqual(result.draft, draft);
    });

    it('drops the image and flips dropped=true when over the limit', () => {
      const draft = {
        text: 'hi',
        image: { base64: 'x'.repeat(5000), name: 'a.png', mime: 'image/png' },
        savedAt: 7,
      };
      const result = applyQuotaFallback(draft, 1000);
      assert.equal(result.dropped, true);
      assert.equal(result.draft.image, null);
      assert.equal(result.draft.text, 'hi');
      assert.equal(result.draft.savedAt, 7);
    });

    it('keeps text-only drafts that exceed the limit (no image to drop)', () => {
      const draft = { text: 'x'.repeat(5000), image: null, savedAt: 1 };
      const result = applyQuotaFallback(draft, 100);
      assert.equal(result.dropped, true);
      assert.equal(result.draft.image, null);
      assert.equal(result.draft.text, 'x'.repeat(5000));
    });
  });

  describe('isTextOnlyOversize', () => {
    it('is false when the draft has an image (even if oversize)', () => {
      const draft = {
        text: 'x'.repeat(500),
        image: { base64: 'y'.repeat(5000), name: 'a.png', mime: 'image/png' },
        savedAt: 1,
      };
      assert.equal(isTextOnlyOversize(draft, 100), false);
    });

    it('is false when text-only draft fits within the limit', () => {
      const draft = { text: 'hello', image: null, savedAt: 1 };
      assert.equal(isTextOnlyOversize(draft, 10_000), false);
    });

    it('is true when text-only draft exceeds the limit', () => {
      const draft = { text: 'x'.repeat(5000), image: null, savedAt: 1 };
      assert.equal(isTextOnlyOversize(draft, 100), true);
    });
  });

  describe('hasContent', () => {
    it('is true when text has non-whitespace', () => {
      assert.equal(hasContent('hi', false), true);
    });

    it('is true when image is attached', () => {
      assert.equal(hasContent('   ', true), true);
    });

    it('is false when both empty', () => {
      assert.equal(hasContent('   ', false), false);
    });
  });

  describe('base64 round trip', () => {
    it('encodes bytes and decodes back to the same values', () => {
      const original = new Uint8Array([0, 1, 2, 250, 251, 252, 253, 254, 255]);
      const encoded = encodeBytesToBase64(original);
      const decoded = decodeBase64ToBytes(encoded);
      assert.deepEqual(Array.from(decoded), Array.from(original));
    });

    it('survives a long binary payload', () => {
      const size = 50_000;
      const bytes = new Uint8Array(size);
      for (let i = 0; i < size; i++) bytes[i] = (i * 31) & 0xff;
      const encoded = encodeBytesToBase64(bytes);
      const decoded = decodeBase64ToBytes(encoded);
      assert.equal(decoded.length, size);
      for (let i = 0; i < size; i += 1337) {
        assert.equal(decoded[i], bytes[i]);
      }
    });
  });

  describe('estimateSerializedLength', () => {
    it('returns the JSON string length for well-formed drafts', () => {
      const draft = { text: 'hi', image: null, savedAt: 1 };
      assert.equal(estimateSerializedLength(draft), JSON.stringify(draft).length);
    });
  });
});

import https from 'https';
import { createWriteStream } from 'fs';
import { type FeedResult, feedResponseSchema } from '#main/updater/types/updater';
import { buildFeedUrl } from '#main/updater/service/channelConfig';
import { MAX_REDIRECTS } from '#main/updater/config/updater';
import { parseJsonTextWith } from '#result';

export function fetchFeed() {
  return new Promise<FeedResult>((resolve) => {
    const url = buildFeedUrl();
    const req = https.get(url, (res) => {
      if (res.statusCode === 204) {
        resolve({ kind: 'up-to-date' });
        return;
      }
      let body = '';
      res.on('data', (chunk: Buffer) => {
        if (body.length < 500 || res.statusCode === 200) body += chunk.toString();
      });
      res.on('end', () => {
        if (res.statusCode !== 200) {
          resolve({ kind: 'error' });
          return;
        }
        const parsed = parseJsonTextWith(body, feedResponseSchema, 'https:update-feed');
        if (parsed.isErr()) {
          resolve({ kind: 'error' });
          return;
        }
        resolve({ kind: 'update', feed: parsed.value });
      });
      res.on('error', (_err) => {
        resolve({ kind: 'error' });
      });
    });
    req.on('error', (_err) => {
      resolve({ kind: 'error' });
    });
  });
}

export function downloadFile(
  url: string,
  dest: string,
  redirectsLeft = MAX_REDIRECTS,
): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (!url.startsWith('https://')) {
      reject(new Error(`Refusing non-HTTPS download URL: ${url.substring(0, 80)}`));
      return;
    }

    const file = createWriteStream(dest);

    const request = https.get(url, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        const location = res.headers.location;
        file.close();
        if (!location) {
          res.resume();
          reject(new Error('Redirect with no location'));
          return;
        }
        if (redirectsLeft <= 0) {
          res.resume();
          reject(new Error('Too many redirects'));
          return;
        }
        res.resume();
        void (async () => {
          try {
            await downloadFile(location, dest, redirectsLeft - 1);
            resolve();
          } catch (err) {
            reject(err);
          }
        })();
        return;
      }
      if (res.statusCode !== 200) {
        file.close();
        reject(new Error(`Download failed: HTTP ${res.statusCode}`));
        return;
      }
      res.pipe(file);
      file.on('finish', () => {
        file.close();
        resolve();
      });
    });
    request.on('error', (err) => {
      file.close();
      reject(err);
    });
  });
}

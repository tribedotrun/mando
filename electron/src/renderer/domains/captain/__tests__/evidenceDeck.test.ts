import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { prepareEvidenceDeck } from '../service/evidenceDeck.ts';

describe('prepareEvidenceDeck', () => {
  it('embeds local deck assets while preserving remote Reveal resources', () => {
    const result = prepareEvidenceDeck({
      html: [
        '<head>',
        '<link href="https://cdn.example/reveal.css" rel="stylesheet">',
        '</head><body><div class="reveal">',
        '<img src="./after.png">',
        '<video poster="poster.jpg"><source src="demo.mp4"></video>',
        '<div style="background-image:url(\'poster.jpg\')"></div>',
        '<script src="https://cdn.example/reveal.js"></script>',
      ].join(''),
      modifiedAtMs: 42,
      assets: [
        { path: 'after.png', mime: 'image/png', base64: 'YWZ0ZXI=' },
        { path: 'poster.jpg', mime: 'image/jpeg', base64: 'cG9zdGVy' },
        { path: 'demo.mp4', mime: 'video/mp4', base64: 'ZGVtbw==' },
      ],
    });

    assert.match(result.html, /src="data:image\/png;base64,YWZ0ZXI="/);
    assert.match(result.html, /poster="data:image\/jpeg;base64,cG9zdGVy"/);
    assert.match(result.html, /src="data:video\/mp4;base64,ZGVtbw=="/);
    assert.match(result.html, /url\('data:image\/jpeg;base64,cG9zdGVy'\)/);
    assert.match(result.html, /href="https:\/\/cdn\.example\/reveal\.css"/);
    assert.match(result.html, /data-mando-deck-viewer/);
    assert.match(result.html, /data-mando-deck-runtime/);
    assert.ok(
      result.html.indexOf('data-mando-deck-runtime') <
        result.html.indexOf('https://cdn.example/reveal.js'),
    );
    assert.equal(result.modifiedAtMs, 42);
  });
});

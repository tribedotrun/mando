import { Linter } from 'eslint';
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import processIsolation from '../process-isolation.mjs';
import tierMatrix from '../tier-matrix.mjs';

const linter = new Linter();
const matrix = [...processIsolation, ...tierMatrix];

function lint(filename, code) {
  return linter.verify(code, matrix, { filename });
}

function expectMessage(messages, fragment) {
  const hit = messages.find((m) => m.message.includes(fragment));
  assert.ok(
    hit,
    `expected a message containing "${fragment}", got: ${JSON.stringify(messages.map((m) => m.message))}`,
  );
}

function expectClean(messages) {
  assert.equal(messages.length, 0, `expected no violations, got: ${JSON.stringify(messages)}`);
}

describe('process isolation', () => {
  it('renderer cannot import from main', () => {
    expectMessage(
      lint('src/renderer/global/runtime/foo.ts', `import { x } from '#main/foo';`),
      'renderer cannot import from main',
    );
  });

  it('main cannot import from renderer', () => {
    expectMessage(
      lint('src/main/index.ts', `import { x } from '#renderer/global/types';`),
      'main cannot import from renderer',
    );
  });

  it('relative imports are banned', () => {
    expectMessage(lint('src/renderer/global/runtime/foo.ts', `import x from './bar';`), 'aliases');
    expectMessage(lint('src/main/daemon/runtime/foo.ts', `import x from '../bar';`), 'aliases');
  });

  it('renderer entry point allows relative imports', () => {
    expectClean(lint('src/renderer/index.tsx', `import './styles.css';`));
  });
});

describe('renderer global: forward-only tier imports', () => {
  it('types cannot import higher tiers', () => {
    expectMessage(
      lint('src/renderer/global/types/foo.ts', `import { x } from '#renderer/global/providers/http';`),
      'cannot import',
    );
  });

  it('providers can import types', () => {
    expectClean(
      lint('src/renderer/global/providers/foo.ts', `import { x } from '#renderer/global/types';`),
    );
  });

  it('providers cannot import repo', () => {
    expectMessage(
      lint('src/renderer/global/providers/foo.ts', `import { x } from '#renderer/global/repo/queryKeys';`),
      'cannot import',
    );
  });

  it('repo can import providers', () => {
    expectClean(
      lint('src/renderer/global/repo/foo.ts', `import { x } from '#renderer/global/providers/http';`),
    );
  });

  it('service cannot import providers or repo', () => {
    expectMessage(
      lint('src/renderer/global/service/foo.ts', `import { x } from '#renderer/global/providers/http';`),
      'cannot import',
    );
    expectMessage(
      lint('src/renderer/global/service/foo.ts', `import { x } from '#renderer/global/repo/queryKeys';`),
      'cannot import',
    );
  });

  it('runtime can import service', () => {
    expectClean(
      lint('src/renderer/global/runtime/foo.ts', `import { x } from '#renderer/global/service/utils';`),
    );
  });

  it('global cannot import from domains', () => {
    expectMessage(
      lint('src/renderer/global/runtime/foo.ts', `import { x } from '#renderer/domains/captain/ui/Foo';`),
      'cannot import from domains',
    );
  });

  it('global cannot import from app', () => {
    expectMessage(
      lint('src/renderer/global/service/foo.ts', `import { x } from '#renderer/app/router';`),
      'cannot import from app',
    );
  });
});

describe('renderer domains: tier isolation', () => {
  it('domain types cannot import higher tiers in own domain', () => {
    expectMessage(
      lint('src/renderer/domains/captain/types/foo.ts', `import { x } from '#renderer/domains/captain/repo/api';`),
      'cannot import',
    );
  });

  it('domain repo can import own types', () => {
    expectClean(
      lint('src/renderer/domains/captain/repo/foo.ts', `import { x } from '#renderer/domains/captain/types/bar';`),
    );
  });

  it('domain ui can import own runtime', () => {
    expectClean(
      lint('src/renderer/domains/captain/ui/Foo.tsx', `import { x } from '#renderer/domains/captain/runtime/useApi';`),
    );
  });

  it('domain service cannot import own providers or repo', () => {
    expectMessage(
      lint('src/renderer/domains/captain/service/foo.ts', `import { x } from '#renderer/global/providers/http';`),
      'pure',
    );
  });

  it('domain cannot import another domain internals', () => {
    expectMessage(
      lint('src/renderer/domains/captain/ui/Foo.tsx', `import { x } from '#renderer/domains/scout/ui/Bar';`),
      'Promote shared code to global',
    );
  });

  it('domain can import global at same or lower tier', () => {
    expectClean(
      lint('src/renderer/domains/captain/repo/foo.ts', `import { x } from '#renderer/global/providers/http';`),
    );
    expectClean(
      lint('src/renderer/domains/captain/runtime/foo.ts', `import { x } from '#renderer/global/repo/queryKeys';`),
    );
  });

  it('domains cannot import from app', () => {
    expectMessage(
      lint('src/renderer/domains/scout/ui/Foo.tsx', `import { x } from '#renderer/app/router';`),
      'cannot import from app',
    );
  });
});

describe('renderer app layer', () => {
  it('app can import global providers', () => {
    expectClean(
      lint('src/renderer/app/routes/Foo.tsx', `import { x } from '#renderer/global/providers/http';`),
    );
  });

  it('app can import global repo', () => {
    expectClean(
      lint('src/renderer/app/routes/Foo.tsx', `import { x } from '#renderer/global/repo/queries';`),
    );
  });

  it('app can import domain ui directly', () => {
    expectClean(
      lint('src/renderer/app/routes/Foo.tsx', `import { x } from '#renderer/domains/captain/ui/CaptainView';`),
    );
  });

  it('app can import global runtime', () => {
    expectClean(
      lint('src/renderer/app/routes/Foo.tsx', `import { x } from '#renderer/global/runtime/useSseSync';`),
    );
  });

  it('app can import global ui', () => {
    expectClean(
      lint('src/renderer/app/routes/Foo.tsx', `import { x } from '#renderer/global/ui/button';`),
    );
  });
});

describe('main process: tier isolation', () => {
  it('main global types cannot import higher tiers', () => {
    expectMessage(
      lint('src/main/global/types/foo.ts', `import { x } from '#main/global/providers/logger';`),
      'cannot import',
    );
  });

  it('main domain cannot import another domain', () => {
    expectMessage(
      lint('src/main/daemon/service/foo.ts', `import { x } from '#main/updater/runtime/updater';`),
      'Promote shared code to global',
    );
  });

  it('main domain can import global', () => {
    expectClean(
      lint('src/main/daemon/runtime/foo.ts', `import { x } from '#main/global/providers/logger';`),
    );
  });
});

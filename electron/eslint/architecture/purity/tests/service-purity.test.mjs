import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/service-purity.mjs';

ruleTester.run('service-purity', rule, {
  valid: [
    { code: `import { x } from '#renderer/global/types';`, filename: 'src/renderer/global/service/utils.ts' },
    { code: `import React from 'react';`, filename: 'src/renderer/domains/captain/runtime/useApi.ts' },
    { code: `import React from 'react';`, filename: 'src/renderer/domains/captain/ui/Foo.tsx' },
  ],
  invalid: [
    { code: `import React from 'react';`, filename: 'src/renderer/global/service/utils.ts', errors: [{ messageId: 'impure' }] },
    { code: `import { useState } from 'react';`, filename: 'src/renderer/domains/captain/service/foo.ts', errors: [{ messageId: 'impure' }] },
    { code: `import { x } from '#renderer/global/providers/http';`, filename: 'src/renderer/global/service/foo.ts', errors: [{ messageId: 'impure' }] },
    { code: `import { x } from '#renderer/global/runtime/useSseSync';`, filename: 'src/renderer/global/service/foo.ts', errors: [{ messageId: 'impure' }] },
    { code: `import { x } from '#renderer/domains/captain/repo/api';`, filename: 'src/renderer/domains/captain/service/foo.ts', errors: [{ messageId: 'impure' }] },
  ],
});

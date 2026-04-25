import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-raw-boundary-text-parse.mjs';

ruleTester.run('architecture/no-raw-boundary-text-parse', rule, {
  valid: [
    {
      code: `const port = mustParsePortNumberText(raw, 'file:daemon.port');`,
      filename: 'src/main/global/runtime/portCheck.ts',
    },
    {
      code: `const parsed = parseLaunchctlPidText(out, 'command:launchctl-list');`,
      filename: 'src/main/global/runtime/portCheck.ts',
    },
    {
      code: `const version = parseNonEmptyText(result.stdout, 'command:claude-version');`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
    },
    {
      code: `const trimmed = fs.readFileSync(path, 'utf-8').trim();`,
      filename: 'src/renderer/global/service/utils.ts',
    },
  ],
  invalid: [
    {
      code: `const port = fs.readFileSync('/tmp/daemon.port', 'utf-8').trim();`,
      filename: 'src/main/onboarding/repo/config.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const port = fs.readFileSync('/tmp/daemon.port', 'utf-8').trim();`,
      filename: 'src/main/global/service/daemonDiscovery.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const raw = await fs.promises.readFile('/tmp/daemon.port', 'utf-8'); const port = raw.trim();`,
      filename: 'src/main/global/service/daemonDiscovery.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const port = parseInt(fs.readFileSync('/tmp/daemon.port', 'utf-8').trim(), 10);`,
      filename: 'src/main/global/runtime/portCheck.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }, { messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const version = execSync('claude --version', { encoding: 'utf-8' }).trim();`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const ver = await run('claude', ['--version'], 1000); if (ver) version = ver.stdout.trim();`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const out = execSync('launchctl list test', { encoding: 'utf-8' }); const pidMatch = out.match(/"PID"\\s*=\\s*(\\d+)/);`,
      filename: 'src/main/global/runtime/portCheck.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const out = execSync('launchctl list test', { encoding: 'utf-8' }); const pidMatch = /"PID"\\s*=\\s*(\\d+)/.exec(out);`,
      filename: 'src/main/global/runtime/portCheck.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const raw = fs.readFileSync('/tmp/daemon.port', 'utf-8'); const alias = raw; const port = alias.trim();`,
      filename: 'src/main/global/service/daemonDiscovery.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const { stdout } = await run('claude', ['--version'], 1000); const version = stdout.trim();`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const clean = (value) => value.trim(); const raw = fs.readFileSync('/tmp/daemon.port', 'utf-8'); clean(raw);`,
      filename: 'src/main/global/service/daemonDiscovery.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const port = Number(fs.readFileSync('/tmp/daemon.port', 'utf-8'));`,
      filename: 'src/main/global/runtime/portCheck.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const normalized = fs.readFileSync('/tmp/daemon.port', 'utf-8').replace(/\\s+/g, '');`,
      filename: 'src/main/global/service/daemonDiscovery.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const version = execFileSync('claude', ['--version'], { encoding: 'utf-8' }).trim();`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const stderr = stderrString(e); const message = stderr.trim();`,
      filename: 'src/main/global/runtime/launchd.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const detail = await response.text().then((text) => text.trim());`,
      filename: 'src/main/global/runtime/daemonTransport.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
    {
      code: `const { stdout } = await execFileAsync('claude', ['--version']); const version = stdout.trim();`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
      errors: [{ messageId: 'useBoundaryTextHelper' }],
    },
  ],
});

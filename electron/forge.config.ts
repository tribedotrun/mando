import type { ForgeConfig } from '@electron-forge/shared-types';
import { FusesPlugin } from '@electron-forge/plugin-fuses';
import { VitePlugin } from '@electron-forge/plugin-vite';
import { MakerDMG } from '@electron-forge/maker-dmg';
import { MakerZIP } from '@electron-forge/maker-zip';
import { PublisherGithub } from '@electron-forge/publisher-github';
import { FuseV1Options, FuseVersion } from '@electron/fuses';

const VALID_MODES = ['production', 'dev', 'prod-local', 'sandbox', 'preview'] as const;
const rawMode = process.env.MANDO_APP_MODE || 'production';
if (!VALID_MODES.includes(rawMode as (typeof VALID_MODES)[number])) {
  throw new Error(`Invalid MANDO_APP_MODE="${rawMode}". Must be: ${VALID_MODES.join(', ')}`);
}
const appMode = rawMode as (typeof VALID_MODES)[number];
const APP_NAMES: Record<string, string> = {
  dev: 'Mando (Dev)',
  'prod-local': 'Mando (Prod Local)',
  sandbox: 'Mando (Sandbox)',
  preview: 'Mando (Preview)',
};
const BUNDLE_IDS: Record<string, string> = {
  dev: 'run.tribe.mando-dev',
  'prod-local': 'run.tribe.mando-prod-local',
  sandbox: 'run.tribe.mando-sandbox',
  preview: 'build.mando.preview',
};
const appName = APP_NAMES[appMode] || 'Mando';
const bundleId = BUNDLE_IDS[appMode] || 'run.tribe.mando';

const rustTargetDir = process.env.MANDO_RUST_TARGET_DIR || '../rust/target/release';

const config: ForgeConfig = {
  packagerConfig: {
    name: appName,
    executableName: appName,
    icon: './assets/icon',
    asar: true,
    appBundleId: bundleId,
    darwinDarkModeSupport: true,
    extendInfo: {},
    extraResource: [`${rustTargetDir}/mando-gw`, `${rustTargetDir}/mando`, './assets'],
    osxSign: {
      optionsForFile: () => ({
        entitlements: 'entitlements/entitlements.mac.plist',
        hardenedRuntime: true,
      }),
    },
    osxNotarize: process.env.CI
      ? undefined // CI notarizes as a separate step for timing visibility
      : {
          keychainProfile: 'mando-notorize',
        },
  },
  makers: [
    new MakerDMG({
      name: appName,
      icon: './assets/icon.icns',
      'icon-size': 80,
      contents: [
        { x: 180, y: 170, type: 'file', path: '' },
        { x: 480, y: 170, type: 'link', path: '/Applications' },
      ],
      additionalDMGOptions: {
        window: { size: { width: 660, height: 400 }, position: { x: 200, y: 120 } },
      },
    }),
    new MakerZIP({}, ['darwin']),
  ],
  publishers: [
    new PublisherGithub({
      repository: { owner: 'tribedotrun', name: 'mando' },
      draft: true,
      prerelease: false,
      tagPrefix: 'v',
    }),
  ],
  plugins: [
    new VitePlugin({
      build: [
        {
          entry: 'src/main/index.ts',
          config: 'vite.main.config.ts',
          target: 'main',
        },
        {
          entry: 'src/preload/index.ts',
          config: 'vite.preload.config.ts',
          target: 'preload',
        },
      ],
      renderer: [
        {
          name: 'main_window',
          config: 'vite.renderer.config.mts',
        },
      ],
    }),
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};

export default config;

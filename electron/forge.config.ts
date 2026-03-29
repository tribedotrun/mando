import type { ForgeConfig } from '@electron-forge/shared-types';
import { VitePlugin } from '@electron-forge/plugin-vite';
import { MakerDMG } from '@electron-forge/maker-dmg';
import { MakerZIP } from '@electron-forge/maker-zip';
import { PublisherGithub } from '@electron-forge/publisher-github';

const VALID_MODES = ['production', 'dev', 'sandbox'] as const;
const rawMode = process.env.MANDO_APP_MODE || 'production';
if (!VALID_MODES.includes(rawMode as (typeof VALID_MODES)[number])) {
  throw new Error(`Invalid MANDO_APP_MODE="${rawMode}". Must be: ${VALID_MODES.join(', ')}`);
}
const appMode = rawMode as (typeof VALID_MODES)[number];
const appName =
  appMode === 'dev' ? 'Mando (Dev)' : appMode === 'sandbox' ? 'Mando (Sandbox)' : 'Mando';
const bundleId =
  appMode === 'dev'
    ? 'run.tribe.mando-dev'
    : appMode === 'sandbox'
      ? 'run.tribe.mando-sandbox'
      : 'run.tribe.mando';

const rustTargetDir = process.env.MANDO_RUST_TARGET_DIR || '../target/release';

const config: ForgeConfig = {
  packagerConfig: {
    name: appName,
    executableName: appName,
    icon: './assets/icon',
    appBundleId: bundleId,
    darwinDarkModeSupport: true,
    extendInfo: {
      NSMicrophoneUsageDescription: 'Mando uses the microphone for voice commands.',
    },
    extraResource: [
      `${rustTargetDir}/mando-gw`,
      `${rustTargetDir}/mando-tg`,
      `${rustTargetDir}/mando`,
      './assets',
    ],
    osxSign: {
      optionsForFile: () => ({
        entitlements: 'entitlements/entitlements.mac.plist',
        hardenedRuntime: true,
      }),
    },
    osxNotarize: {
      keychainProfile: 'mando-notorize',
    },
  },
  makers: [
    new MakerDMG({ name: appName, icon: './assets/icon.icns' }),
    new MakerZIP({}, ['darwin']),
  ],
  publishers: [
    new PublisherGithub({
      repository: { owner: 'tribedotrun', name: 'mando-private' },
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
  ],
};

export default config;

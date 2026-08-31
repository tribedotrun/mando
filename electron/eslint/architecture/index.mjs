import processIsolation from './imports/process-isolation.mjs';
import tierMatrix from './imports/tier-matrix.mjs';

// Architecture invariant rules (.claude/skills/s-arch/invariants.md).
import noDirectLocalStorage from './rules/no-direct-localStorage.mjs';
import daemonSyncPolicy from './rules/daemon-sync-policy.mjs';
import queryKeyFactory from './rules/query-key-factory.mjs';
import noChannelWideCleanup from './rules/no-channel-wide-cleanup.mjs';
import noReactQueryCreatorsOutsideRepo from './rules/no-react-query-creators-outside-repo.mjs';
import noNativeBridgeOutsideProviders from './rules/no-native-bridge-outside-providers.mjs';
import toastOnlyInUi from './rules/toast-only-in-ui.mjs';
import noRawBoundaryJsonParse from './rules/no-raw-boundary-json-parse.mjs';
import noRawBoundaryTextParse from './rules/no-raw-boundary-text-parse.mjs';
import noRawContractIpcSend from './rules/no-raw-contract-ipc-send.mjs';
import preferJsonPersistenceSlots from './rules/prefer-json-persistence-slots.mjs';
import requireMultipartShadowBody from './rules/require-multipart-shadow-body.mjs';

const architecturePlugin = {
  rules: {
    'daemon-sync-policy': daemonSyncPolicy,
    'no-direct-localStorage': noDirectLocalStorage,
    'query-key-factory': queryKeyFactory,
    'no-channel-wide-cleanup': noChannelWideCleanup,
    'no-react-query-creators-outside-repo': noReactQueryCreatorsOutsideRepo,
    'no-native-bridge-outside-providers': noNativeBridgeOutsideProviders,
    'toast-only-in-ui': toastOnlyInUi,
    'no-raw-boundary-json-parse': noRawBoundaryJsonParse,
    'no-raw-boundary-text-parse': noRawBoundaryTextParse,
    'no-raw-contract-ipc-send': noRawContractIpcSend,
    'prefer-json-persistence-slots': preferJsonPersistenceSlots,
    'require-multipart-shadow-body': requireMultipartShadowBody,
  },
};

export default [
  ...processIsolation,
  ...tierMatrix,
  { plugins: { architecture: architecturePlugin } },
  {
    // Renderer-scoped architecture rules.
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'architecture/daemon-sync-policy': 'error',
      'architecture/no-direct-localStorage': 'error',
      'architecture/query-key-factory': 'error',
      'architecture/no-react-query-creators-outside-repo': 'error',
      'architecture/no-native-bridge-outside-providers': 'error',
    },
  },
  {
    // Channel-wide cleanup (removeAllListeners / remove*Listeners) is
    // banned end-to-end — main, preload, and renderer call sites.
    files: [
      'src/main/**/*.ts',
      'src/preload/**/*.ts',
      'src/renderer/**/*.ts',
      'src/renderer/**/*.tsx',
    ],
    rules: {
      'architecture/no-channel-wide-cleanup': 'error',
    },
  },
  {
    // Main process lifecycle rules.
    files: ['src/main/**/*.ts'],
    rules: {
      'architecture/no-raw-boundary-text-parse': 'error',
      'architecture/no-raw-contract-ipc-send': 'error',
    },
  },
  {
    // Boundary JSON parsing must stay inside the shared result helpers across
    // every Electron process and the shared IPC contract layer.
    files: [
      'src/main/**/*.ts',
      'src/preload/**/*.ts',
      'src/renderer/**/*.ts',
      'src/renderer/**/*.tsx',
      'src/shared/**/*.ts',
    ],
    rules: {
      'architecture/no-raw-boundary-json-parse': 'error',
    },
  },
  {
    // Renderer feedback, persistence, multipart, and observability boundaries.
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'architecture/toast-only-in-ui': 'error',
      'architecture/prefer-json-persistence-slots': 'error',
      'architecture/require-multipart-shadow-body': 'error',
      // D5 bans every `console.*` call in the renderer; no escape hatches.
      // Diagnostic output must go through the global logger
      // (`#renderer/global/service/logger`) so it threads into observability
      // instead of dying in the devtools console.
      'no-console': 'error',
    },
  },
];

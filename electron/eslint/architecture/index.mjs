import purity from './purity/plugin.mjs';
import processIsolation from './imports/process-isolation.mjs';
import tierMatrix from './imports/tier-matrix.mjs';

// Architecture invariant rules (.claude/skills/s-arch/invariants.md).
import noDirectLocalStorage from './rules/no-direct-localStorage.mjs';
import noDomEventBus from './rules/no-dom-event-bus.mjs';
import queryKeyFactory from './rules/query-key-factory.mjs';
import noChannelWideCleanup from './rules/no-channel-wide-cleanup.mjs';
import preloadSubscribeReturnsUnsubscribe from './rules/preload-subscribe-returns-unsubscribe.mjs';
import mainCompositionOnly from './rules/main-composition-only.mjs';
import noSyncIoOutsideServices from './rules/no-sync-io-outside-services.mjs';
import noModuleScopeMutableState from './rules/no-module-scope-mutable-state.mjs';
import noReactQueryCreatorsOutsideRepo from './rules/no-react-query-creators-outside-repo.mjs';
import noNativeBridgeOutsideProviders from './rules/no-native-bridge-outside-providers.mjs';
import noWindowReload from './rules/no-window-reload.mjs';
import toastOnlyInUi from './rules/toast-only-in-ui.mjs';
import noModuleScopeMutableRenderer from './rules/no-module-scope-mutable-renderer.mjs';
import noRawBoundaryJsonParse from './rules/no-raw-boundary-json-parse.mjs';
import noRawBoundaryTextParse from './rules/no-raw-boundary-text-parse.mjs';
import noRawContractIpcSend from './rules/no-raw-contract-ipc-send.mjs';
import preferJsonPersistenceSlots from './rules/prefer-json-persistence-slots.mjs';
import requireMultipartShadowBody from './rules/require-multipart-shadow-body.mjs';
import noMailboxProp from './rules/no-mailbox-prop.mjs';
import noQueryAsProp from './rules/no-query-as-prop.mjs';
import hookArity from './rules/hook-arity.mjs';
import zustandFineGrained from './rules/zustand-fine-grained.mjs';
import noLeafControlBagProp from './rules/no-leaf-control-bag-prop.mjs';

const architecturePlugin = {
  rules: {
    'no-direct-localStorage': noDirectLocalStorage,
    'no-dom-event-bus': noDomEventBus,
    'query-key-factory': queryKeyFactory,
    'no-channel-wide-cleanup': noChannelWideCleanup,
    'preload-subscribe-returns-unsubscribe': preloadSubscribeReturnsUnsubscribe,
    'main-composition-only': mainCompositionOnly,
    'no-sync-io-outside-services': noSyncIoOutsideServices,
    'no-module-scope-mutable-state': noModuleScopeMutableState,
    'no-react-query-creators-outside-repo': noReactQueryCreatorsOutsideRepo,
    'no-native-bridge-outside-providers': noNativeBridgeOutsideProviders,
    'no-window-reload': noWindowReload,
    'toast-only-in-ui': toastOnlyInUi,
    'no-module-scope-mutable-renderer': noModuleScopeMutableRenderer,
    'no-raw-boundary-json-parse': noRawBoundaryJsonParse,
    'no-raw-boundary-text-parse': noRawBoundaryTextParse,
    'no-raw-contract-ipc-send': noRawContractIpcSend,
    'prefer-json-persistence-slots': preferJsonPersistenceSlots,
    'require-multipart-shadow-body': requireMultipartShadowBody,
    'no-mailbox-prop': noMailboxProp,
    'no-query-as-prop': noQueryAsProp,
    'hook-arity': hookArity,
    'zustand-fine-grained': zustandFineGrained,
    'no-leaf-control-bag-prop': noLeafControlBagProp,
  },
};

export default [
  ...purity,
  ...processIsolation,
  ...tierMatrix,
  { plugins: { architecture: architecturePlugin } },
  {
    // Renderer-scoped architecture rules.
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'architecture/no-direct-localStorage': 'error',
      'architecture/no-dom-event-bus': 'error',
      'architecture/query-key-factory': 'error',
      'architecture/no-react-query-creators-outside-repo': 'error',
      'architecture/no-native-bridge-outside-providers': 'error',
      'architecture/no-window-reload': 'error',
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
    // Preload type files (narrow target).
    files: ['src/preload/types/api.ts', 'src/preload/types/api-channel-map.ts'],
    rules: {
      'architecture/preload-subscribe-returns-unsubscribe': 'error',
    },
  },
  {
    // Main process lifecycle rules.
    files: ['src/main/**/*.ts'],
    rules: {
      'architecture/no-sync-io-outside-services': 'error',
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
    // Main bootstrap — composition-only (covers its own module-scope
    // mutable-state ban so `no-module-scope-mutable-state` below does
    // not double-report on the same declaration).
    files: ['src/main/index.ts'],
    rules: {
      'architecture/main-composition-only': 'error',
    },
  },
  {
    // Main lifecycle module — no module-scope mutable state.
    files: ['src/main/global/runtime/lifecycle.ts', 'src/main/updater/runtime/updater.ts'],
    rules: {
      'architecture/no-module-scope-mutable-state': 'error',
    },
  },
  {
    // Step D guardrails — toast/console/mutable-state in the renderer.
    // These rules are enforced at error level after all violations were drained.
    // See .ai/plans/architecture-invariants/step-D-electron-guardrails.md.
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'architecture/toast-only-in-ui': 'error',
      'architecture/no-module-scope-mutable-renderer': 'error',
      'architecture/prefer-json-persistence-slots': 'error',
      'architecture/require-multipart-shadow-body': 'error',
      'architecture/no-mailbox-prop': 'error',
      'architecture/no-query-as-prop': 'error',
      'architecture/hook-arity': 'error',
      'architecture/zustand-fine-grained': 'error',
      'architecture/no-leaf-control-bag-prop': 'error',
      // D5 bans every `console.*` call in the renderer; no escape hatches.
      // Diagnostic output must go through the global logger
      // (`#renderer/global/service/logger`) so it threads into observability
      // instead of dying in the devtools console.
      'no-console': 'error',
    },
  },
];

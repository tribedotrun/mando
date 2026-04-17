import { UI_FILE_GLOB, ALL_TS } from '../../shared/constants.mjs';
import noBusinessLogicInUi from './rules/no-business-logic-in-ui.mjs';
import noNetworkInUi from './rules/no-network-in-ui.mjs';
import noIpcInUi from './rules/no-ipc-in-ui.mjs';
import servicePurity from './rules/service-purity.mjs';
import appPurity from './rules/app-purity.mjs';
import barrelDiscipline from './rules/barrel-discipline.mjs';
import tsxComponentsOnly from './rules/tsx-components-only.mjs';

const plugin = {
  rules: {
    'no-business-logic-in-ui': noBusinessLogicInUi,
    'no-network-in-ui': noNetworkInUi,
    'no-ipc-in-ui': noIpcInUi,
    'service-purity': servicePurity,
    'app-purity': appPurity,
    'barrel-discipline': barrelDiscipline,
    'tsx-components-only': tsxComponentsOnly,
  },
};

export default [
  { plugins: { arch: plugin } },
  {
    files: [UI_FILE_GLOB],
    rules: {
      'arch/no-business-logic-in-ui': 'error',
      'arch/no-network-in-ui': 'error',
      'arch/no-ipc-in-ui': 'error',
    },
  },
  {
    files: ['src/**/*.tsx'],
    rules: {
      'arch/tsx-components-only': 'error',
    },
  },
  {
    files: ALL_TS,
    rules: {
      'arch/service-purity': 'error',
      'arch/app-purity': 'error',
      'arch/barrel-discipline': 'error',
    },
  },
];

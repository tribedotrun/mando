import { ALL_TS } from '../shared/constants.mjs';

const EFFECT_MSG =
  '{{name}} is banned. Use useMountEffect, useQuery, derived state, or event handlers.';
const HOVER_MSG = 'Inline hover handlers are banned. Use CSS :hover or Tailwind hover: classes.';

const bannedEffect = (name) => [
  {
    selector: `CallExpression[callee.name="${name}"]`,
    message: EFFECT_MSG.replace('{{name}}', name),
  },
  {
    selector: `CallExpression[callee.object.name="React"][callee.property.name="${name}"]`,
    message: EFFECT_MSG.replace('{{name}}', name),
  },
];

const bannedHandler = (name) => ({
  selector: `JSXAttribute[name.name="${name}"]`,
  message: HOVER_MSG,
});

export default [
  {
    files: ALL_TS,
    ignores: [
      'src/renderer/global/runtime/useMountEffect.ts',
      'src/renderer/domains/captain/runtime/useDraft.ts',
      'src/renderer/domains/captain/terminal/runtime/useFeedbackTerminalOrchestration.ts',
      'src/renderer/domains/sessions/runtime/useStickyScroll.ts',
      'src/renderer/domains/sessions/runtime/useTranscriptEventsStream.ts',
    ],
    rules: {
      'no-restricted-syntax': [
        'error',
        ...bannedEffect('useEffect'),
        ...bannedEffect('useLayoutEffect'),
        bannedHandler('onMouseEnter'),
        bannedHandler('onMouseLeave'),
        bannedHandler('onMouseOver'),
        bannedHandler('onMouseOut'),
        bannedHandler('onPointerEnter'),
        bannedHandler('onPointerLeave'),
        bannedHandler('onPointerOver'),
        bannedHandler('onPointerOut'),
      ],
    },
  },
];

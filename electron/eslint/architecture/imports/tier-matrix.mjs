import { RENDERER_DOMAINS, RENDERER_TIERS, MAIN_DOMAINS, MAIN_TIERS } from '../../shared/constants.mjs';
import { BAN_RELATIVE, BAN_MAIN, BAN_RENDERER, group, restrictImports } from '../../shared/helpers.mjs';

const SKILL_REF = ' See s-arch skill.';

const RENDERER_ALLOWED = {
  types: [],
  config: ['types'],
  providers: ['types', 'config'],
  repo: ['types', 'config', 'providers'],
  service: ['types', 'config'],
  runtime: ['types', 'config', 'providers', 'repo', 'service'],
  ui: ['types', 'service', 'runtime'],
};

const MAIN_ALLOWED = {
  types: [],
  config: ['types'],
  providers: ['types', 'config'],
  repo: ['types', 'config', 'providers'],
  service: ['types', 'config'],
  runtime: ['types', 'config', 'providers', 'repo', 'service'],
};

function bannedTiers(tier, allowed, allTiers, prefix) {
  const allowedSet = new Set(allowed[tier]);
  return allTiers
    .filter((t) => t !== tier && !allowedSet.has(t))
    .map((t) =>
      group(
        [`${prefix}/${t}/**`],
        `${tier} cannot import ${t}.${SKILL_REF}`,
      ),
    );
}

function rendererGlobalRules() {
  return RENDERER_TIERS.map((tier) => ({
    files: [`src/renderer/global/${tier}/**/*.ts`, `src/renderer/global/${tier}/**/*.tsx`],
    rules: restrictImports(
      BAN_RELATIVE,
      BAN_MAIN,
      ...bannedTiers(tier, RENDERER_ALLOWED, RENDERER_TIERS, '#renderer/global'),
      group(['#renderer/domains/**'], `global/${tier} cannot import from domains.${SKILL_REF}`),
      group(['#renderer/app/**'], `global/${tier} cannot import from app.${SKILL_REF}`),
    ),
  }));
}

function rendererDomainRules() {
  return RENDERER_DOMAINS.flatMap((domain) =>
    RENDERER_TIERS.map((tier) => {
      const domainPrefix = `#renderer/domains/${domain}`;
      const bans = [
        BAN_RELATIVE,
        BAN_MAIN,
        ...bannedTiers(tier, RENDERER_ALLOWED, RENDERER_TIERS, domainPrefix),
        group(['#renderer/app/**'], `Domains cannot import from app.${SKILL_REF}`),
      ];

      if (tier === 'service') {
        bans.push(
          group(
            ['#renderer/global/providers/**', '#renderer/global/repo/**', '#renderer/global/runtime/**', '#renderer/global/ui/**'],
            `Service files must be pure: no providers, no repo, no runtime, no UI from global.${SKILL_REF}`,
          ),
        );
      }

      if (tier === 'ui') {
        bans.push(
          group(
            ['#renderer/global/providers/**'],
            `UI files cannot import providers directly. Use runtime hooks.${SKILL_REF}`,
          ),
          group(
            ['#renderer/global/repo/**'],
            `UI files cannot import repo directly. Use runtime hooks.${SKILL_REF}`,
          ),
          group(
            ['#renderer/global/config/**'],
            `UI files cannot import config directly. Use service or runtime.${SKILL_REF}`,
          ),
        );
      }

      for (const other of RENDERER_DOMAINS) {
        if (other === domain) continue;
        bans.push(
          group(
            [`#renderer/domains/${other}/**`],
            `Domain "${domain}" cannot import "${other}" internals. Promote shared code to global.${SKILL_REF}`,
          ),
        );
      }

      return {
        files: [
          `src/renderer/domains/${domain}/${tier}/**/*.ts`,
          `src/renderer/domains/${domain}/${tier}/**/*.tsx`,
        ],
        rules: restrictImports(...bans),
      };
    }),
  );
}

function rendererCaptainTerminalRules() {
  return RENDERER_TIERS.map((tier) => {
    const bans = [
      BAN_RELATIVE,
      BAN_MAIN,
      ...bannedTiers(tier, RENDERER_ALLOWED, RENDERER_TIERS, '#renderer/domains/captain/terminal'),
      group(['#renderer/app/**'], `Domains cannot import from app.${SKILL_REF}`),
    ];

    if (tier === 'service') {
      bans.push(
        group(
          ['#renderer/global/providers/**', '#renderer/global/repo/**', '#renderer/global/runtime/**', '#renderer/global/ui/**'],
          `Service files must be pure: no providers, no repo, no runtime, no UI from global.${SKILL_REF}`,
        ),
      );
    }

    if (tier === 'ui') {
      bans.push(
        group(['#renderer/global/providers/**'], `UI files cannot import providers directly. Use runtime hooks.${SKILL_REF}`),
        group(['#renderer/global/repo/**'], `UI files cannot import repo directly. Use runtime hooks.${SKILL_REF}`),
        group(['#renderer/global/config/**'], `UI files cannot import config directly. Use service or runtime.${SKILL_REF}`),
      );
    }

    for (const other of RENDERER_DOMAINS) {
      if (other === 'captain') continue;
      bans.push(
        group(
          [`#renderer/domains/${other}/**`],
          `captain/terminal cannot import "${other}" internals. Promote shared code to global.${SKILL_REF}`,
        ),
      );
    }

    return {
      files: [
        `src/renderer/domains/captain/terminal/${tier}/**/*.ts`,
        `src/renderer/domains/captain/terminal/${tier}/**/*.tsx`,
      ],
      rules: restrictImports(...bans),
    };
  });
}

function rendererAppRules() {
  return [
    {
      files: ['src/renderer/app/**/*.ts', 'src/renderer/app/**/*.tsx'],
      rules: restrictImports(BAN_RELATIVE, BAN_MAIN),
    },
    {
      files: ['src/renderer/index.tsx'],
      rules: { 'no-restricted-imports': 'off' },
    },
  ];
}

function mainGlobalRules() {
  return MAIN_TIERS.map((tier) => ({
    files: [`src/main/global/${tier}/**/*.ts`],
    rules: restrictImports(
      BAN_RELATIVE,
      BAN_RENDERER,
      ...bannedTiers(tier, MAIN_ALLOWED, MAIN_TIERS, '#main/global'),
    ),
  }));
}

function mainDomainRules() {
  return MAIN_DOMAINS.flatMap((domain) =>
    MAIN_TIERS.map((tier) => {
      const bans = [
        BAN_RELATIVE,
        BAN_RENDERER,
        ...bannedTiers(tier, MAIN_ALLOWED, MAIN_TIERS, `#main/${domain}`),
      ];

      for (const other of MAIN_DOMAINS) {
        if (other === domain) continue;
        bans.push(
          group(
            [`#main/${other}/**`],
            `Domain "${domain}" cannot import "${other}" internals. Promote shared code to global.${SKILL_REF}`,
          ),
        );
      }

      return {
        files: [`src/main/${domain}/${tier}/**/*.ts`],
        rules: restrictImports(...bans),
      };
    }),
  );
}

export default [
  ...rendererGlobalRules(),
  ...rendererDomainRules(),
  ...rendererCaptainTerminalRules(),
  ...rendererAppRules(),
  ...mainGlobalRules(),
  ...mainDomainRules(),
];

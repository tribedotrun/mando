const LANG_ALIASES: Record<string, string> = {
  ts: 'typescript',
  js: 'javascript',
  py: 'python',
  sh: 'bash',
  shell: 'bash',
  yml: 'yaml',
  md: 'markdown',
  rs: 'rust',
};

export function normalizeLang(lang: string): string {
  return LANG_ALIASES[lang] ?? lang;
}

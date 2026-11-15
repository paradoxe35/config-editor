// The texts the random actions type, paste and set: plain words, YAML
// pieces, and multi-byte text (accents, CJK, emoji, a family emoji joined
// by zero-width joiners, a combining accent).

export const WORDS = ['a', 'x', 'key', 'value', 'listen', 'port', '42', 'true', 'null', ' ', '  '];
export const PIECES = [': ', '- ', '# note', 'name: n', '- item', '[1, 2]', '{a: 1}', '"q"', "'s'", '|', '&a', '*a', ':', '-'];
export const WIDE = ['é', 'ß', '日本', '中文字', '😀', '👍🏽', '👨‍👩‍👧', 'é', 'Ω≈ç', '🇫🇷'];

// IME compositions: the steps a user's input method shows, then the text
// it commits.
export const COMPOSITIONS = [
  { steps: ['n', 'に', 'にh', 'にほ', 'にほん'], commit: '日本' },
  { steps: ['k', 'ka', 'か'], commit: 'か' },
  { steps: ['z', 'zh', 'zho', 'zhong'], commit: '中' },
  { steps: ['ㅎ', '하', '한'], commit: '한' },
  { steps: ['e', 'é'], commit: 'é' },
];

// Whole documents a host sets (YAML: the single-editor pages edit YAML).
const LONG =
  Array.from({ length: 160 }, (_, index) =>
    index % 10 === 0 ? `group_${index}:\n  name: g${index}\n  items:\n    - one\n    - two` : `key_${index}: value ${index} 😀`,
  ).join('\n') + '\n';

export const YAML_DOCS = [
  '# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n',
  'listen: 9000\n',
  'server:\n  host: localhost\n  ports:\n    - 80\n    - 443\n  tls:\n    enabled: true\nname: 日本 😀\n',
  'clients:\n  acme:\n    secret: «secret 3f2a»\n',
  '',
  LONG,
];

export const TOML_DOCS = ['[limits]\nmax_pending = 512\n', 'title = "x"\n[server]\nport = 1\nname = "日本"\n', ''];
export const JSON_DOCS = ['{\n  "clients": {}\n}\n', '{\n  "a": [1, 2],\n  "b": {"c": "😀"}\n}\n', '{}\n'];

export function docsFor(language) {
  if (language === 'toml') return TOML_DOCS;
  if (language === 'json') return JSON_DOCS;
  return YAML_DOCS;
}

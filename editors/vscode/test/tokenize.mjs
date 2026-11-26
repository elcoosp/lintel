// The T-16 acceptance check (verbatim: "JSX highlighted inside .lintl.php"):
// the grammar runs through the same tokenizer engine VS Code uses
// (vscode-textmate + vscode-oniguruma) and the assertions pin the behavior,
// not just the JSON's shape:
//   - a component and an HTML element at expression-start positions get
//     markup scopes (entity.name.tag.lintl);
//   - a `{...}` child keeps its PHP expression scope;
//   - `$a < $b` (operand-position `<`) never becomes markup.
// CI's `editors` job runs this on every push (setup-node + npm test).
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import oniguruma from 'vscode-oniguruma';
const { loadWASM } = oniguruma;
import textmate from 'vscode-textmate';
const { Registry } = textmate;

const here = dirname(fileURLToPath(import.meta.url));

// The oniguruma wasm ships with vscode-oniguruma in node_modules.
await loadWASM(await readFile(join(here, '../node_modules/vscode-oniguruma/release/onig.wasm')));

const grammar = JSON.parse(await readFile(join(here, '../grammars/lintl.tmLanguage.json'), 'utf8'));

const registry = new Registry({
  onigLib: Promise.resolve(oniguruma),
  loadGrammar: async (scopeName) =>
    scopeName === grammar.scopeName ? grammar : null,
});

const grammar_ = await registry.loadGrammar('source.lintl.php');

function tokenize(source) {
  const lines = source.split('\n');
  let ruleStack = null;
  const scopes = [];
  for (const line of lines) {
    const result = grammar_.tokenizeLine(line, ruleStack);
    ruleStack = result.ruleStack;
    scopes.push(result.tokens);
  }
  return scopes;
}

let failures = 0;
function check(label, ok, detail = '') {
  if (ok) {
    console.log(`ok: ${label}`);
  } else {
    failures += 1;
    console.error(`FAIL: ${label}${detail ? ` — ${detail}` : ''}`);
  }
}

function line_tokens(scopes, index) {
  return scopes[index]
    .map((t) => t.scopes.join(' '))
    .join('\n');
}

// 1. A component invocation is markup.
const component = tokenize([
  '<?php',
  'return <Card title="Hi"><p>Body</p></Card>;',
  '',
].join('\n'));
check(
  'component tag gets entity.name.tag.lintl',
  line_tokens(component, 1).includes('entity.name.tag.lintl'),
  line_tokens(component, 1),
);
check(
  'the component open bracket is lintl punctuation',
  line_tokens(component, 1).includes('punctuation.definition.tag.begin.lintl'),
  line_tokens(component, 1),
);

// 2. An HTML element at expression start is markup too.
const html = tokenize('<?php\nreturn <div class="hero">hi</div>;\n');
check(
  'html element gets entity.name.tag.lintl',
  line_tokens(html, 1).includes('entity.name.tag.lintl'),
  line_tokens(html, 1),
);

// 3. A `{expr}` child stays PHP.
const expr = tokenize('<?php\nreturn <b>{$name}</b>;\n');
check(
  'a {expr} child embeds php scopes',
  line_tokens(expr, 1).includes('meta.embedded.expression.lintl'),
  line_tokens(expr, 1),
);

// 4. `<` after an operand is a comparison, never markup.
const compare = tokenize('<?php\n$ok = $a < $b;\n');
check(
  'operand-position < stays a comparison',
  !line_tokens(compare, 1).includes('entity.name.tag.lintl'),
  line_tokens(compare, 1),
);

// 5. The multiline element keeps nesting across lines (children scope runs
// past the open tag's line).
const multiline = tokenize('<?php\nreturn <main>\n    <Row>one</Row>\n</main>;\n');
check(
  'a nested row inside a multiline element is markup',
  line_tokens(multiline, 2).includes('entity.name.tag.lintl'),
  line_tokens(multiline, 2),
);

if (failures > 0) {
  console.error(`grammar: ${failures} check(s) failed`);
  process.exit(1);
}
console.log('grammar: all checks green');

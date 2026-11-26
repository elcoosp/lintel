// The T-13 playground logic: one module, zero external requests, so the
// service worker (sw.js) can make the page fully offline after first load.
// The compiler is the committed wasm bundle in ./assets/, built by
// `make playground` from crates/lintl-wasm.
import init, { compile_source, lintl_version } from './assets/lintl_wasm.js';

const $ = (id) => document.getElementById(id);

const EXAMPLES = {
  'typed-component': {
    label: 'Component with a typed parameter',
    target: 'string',
    source: `<?php

declare(strict_types=1);

function Card(string $title): Html
{
    return <div class="card">
        <h2>{$title}</h2>
        <p>Compiled to plain string concat — the typed parameter escapes directly.</p>
    </div>;
}

echo Card(title: 'Hello <world> & friends');
`,
  },
  'streaming': {
    label: 'Streaming generator (stream target)',
    target: 'stream',
    source: `<?php

declare(strict_types=1);

// The stream target lowers markup to generator closures that capture the
// locals their body references — this component streams chunk by chunk.
function Page(string $title, array $items): \\Generator
{
    return <main class="s5">
        <h1>{$title}</h1>
        {items_list($items)}
    </main>;
}

function items_list(array $items): \\Generator
{
    return <ul>
        {items_map($items)}
    </ul>;
}

function items_map(array $items): array
{
    return array_map(fn ($i) => <li class="row">{$i}</li>, $items);
}

foreach (Page(title: 'Streaming TTFB', items: ['one', 'two', 'three']) as $chunk) {
    echo $chunk;
}
`,
  },
  'attributes': {
    label: 'Attributes: URLs, classes, styles, srcset',
    target: 'string',
    source: `<?php

declare(strict_types=1);

$href = 'https://lintl.sh/docs?x=1';
$set = '/a.jpg 1x, /a@2x.jpg 2x';

return <nav>
    <a href={$href} class={["btn", "btn-primary"]}>Docs</a>
    <img src="/a.jpg" srcset={$set} alt="demo" />
    <hr class={null} data-on={true} style={["color: red", "margin: 0"]} />
</nav>;
`,
  },
  'xss-gallery': {
    label: 'Escaping: every payload neutralized',
    target: 'string',
    source: `<?php

declare(strict_types=1);

// Strings escape by default — the payloads render as text, never as markup.
$payloads = [
    '<script>alert(1)</script>',
    '<img src=x onerror=alert(2)>',
    'javascript:alert(3)',
    '</div><script>inject()</script>',
];

return <ul class="xss-gallery">
    {array_map(fn ($p) => <li>{\\Lintl\\c($p)}</li>, $payloads)}
    <li><a href={\\Lintl\\u('href', $payloads[2])}>{'scheme-allowlisted link'}</a></li>
</ul>;
`,
  },
  'validation': {
    label: 'Validation: dynamic on* handler (LNL0301)',
    target: 'string',
    source: `<?php

declare(strict_types=1);

// Dynamic event handlers are rejected at compile time — see the panel below.
$handler = 'track("cta")';

return <button onclick={$handler}>go</button>;
`,
  },
  'rfc-native': {
    label: 'RFC lowering (--target=rfc)',
    target: 'rfc',
    source: `<?php

declare(strict_types=1);

// The rfc target lowers to the native RFC's \\Markup\\Element shape.
function Row(string $label, int $n): \\Markup\\Element
{
    return <tr class="row"><td>{$label}</td><td>{(int)$n}</td></tr>;
}

echo Row(label: 'cells', n: 42);
`,
  },
};

// ---- URL permalinks ---------------------------------------------------------

function encode_state(source, target) {
  const bytes = new TextEncoder().encode(source);
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  const body = btoa(bin).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
  return `#src=${body}&t=${encodeURIComponent(target)}`;
}

function decode_state() {
  const hash = location.hash.startsWith('#') ? location.hash.slice(1) : '';
  const params = new URLSearchParams(hash);
  const src = params.get('src');
  if (src === null) return null;
  let b64 = src.replaceAll('-', '+').replaceAll('_', '/');
  while (b64.length % 4 !== 0) b64 += '=';
  try {
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    const source = new TextDecoder().decode(bytes);
    const target = params.get('t') || 'string';
    return ['string', 'stream', 'rfc'].includes(target) ? { source, target } : null;
  } catch {
    return null;
  }
}

// ---- toast ------------------------------------------------------------------

let toast_timer = null;
function toast(message) {
  let el = $('toast');
  if (el === null) {
    el = document.createElement('div');
    el.id = 'toast';
    el.setAttribute('role', 'status');
    document.body.append(el);
  }
  el.textContent = message;
  el.classList.add('show');
  clearTimeout(toast_timer);
  toast_timer = setTimeout(() => el.classList.remove('show'), 1600);
}

// ---- rendering --------------------------------------------------------------

function render_diagnostics(entries) {
  const list = $('diagnostics');
  const badge = $('diag-count');
  list.replaceChildren();
  const errors = entries.filter((d) => d.severity === 'error').length;
  for (const d of entries) {
    const li = document.createElement('li');
    li.className = `diag ${d.severity}`;
    const head = document.createElement('div');
    head.className = 'diag-head';
    const sev = document.createElement('span');
    sev.className = 'sev';
    sev.textContent = d.severity;
    const code = document.createElement('code');
    code.textContent = d.code;
    const where = document.createElement('span');
    where.className = 'where';
    where.textContent = `line ${d.line}, col ${d.col}`;
    head.append(sev, code, where);
    const body = document.createElement('pre');
    body.textContent = d.rendered;
    li.append(head, body);
    if (d.help) {
      const help = document.createElement('div');
      help.className = 'help';
      help.textContent = `help: ${d.help}`;
      li.append(help);
    }
    list.append(li);
  }
  const total = entries.length;
  badge.hidden = total === 0;
  badge.textContent = String(total);
  badge.classList.toggle('warn-only', errors === 0 && total > 0);
}

function compile_now() {
  const source = $('source').value;
  const target = $('target').value;
  let envelope;
  try {
    envelope = JSON.parse(compile_source(source, target));
  } catch (error) {
    $('compiled').classList.add('error');
    $('compiled-code').textContent = `playground error: ${error}`;
    return;
  }
  render_diagnostics(envelope.diagnostics);
  const out = $('compiled');
  out.classList.toggle('error', !envelope.ok);
  if (envelope.error !== null) {
    $('compiled-code').textContent = envelope.error;
    return;
  }
  $('compiled-code').textContent = envelope.php ?? '';
}

// ---- wiring -----------------------------------------------------------------

function populate_examples() {
  const select = $('example');
  for (const [key, example] of Object.entries(EXAMPLES)) {
    const option = document.createElement('option');
    option.value = key;
    option.textContent = example.label;
    select.append(option);
  }
  select.addEventListener('change', () => {
    const example = EXAMPLES[select.value];
    if (example === undefined) return;
    $('source').value = example.source;
    $('target').value = example.target;
    history.replaceState(null, '', location.pathname + location.search);
    compile_now();
  });
}

async function copy_compiled() {
  const text = $('compiled-code').textContent;
  try {
    await navigator.clipboard.writeText(text);
    toast('Compiled PHP copied');
  } catch {
    const scratch = document.createElement('textarea');
    scratch.value = text;
    document.body.append(scratch);
    scratch.select();
    document.execCommand('copy');
    scratch.remove();
    toast('Compiled PHP copied');
  }
}

async function copy_permalink() {
  const link = new URL(encode_state($('source').value, $('target').value), location.href);
  try {
    await navigator.clipboard.writeText(link.href);
    toast('Permalink copied');
  } catch {
    location.hash = encode_state($('source').value, $('target').value).slice(1);
    toast('Permalink in the address bar');
  }
}

async function main() {
  populate_examples();

  const state = decode_state();
  if (state !== null) {
    $('source').value = state.source;
    $('target').value = state.target;
  } else {
    $('source').value = EXAMPLES['typed-component'].source;
    $('target').value = EXAMPLES['typed-component'].target;
  }

  $('source').addEventListener('input', () => {
    clearTimeout(main.timer);
    main.timer = setTimeout(compile_now, 150);
  });
  $('target').addEventListener('change', compile_now);
  $('copy').addEventListener('click', copy_compiled);
  $('share').addEventListener('click', copy_permalink);

  // The exports need the instance in place first — every wrapper reads the
  // module-level `wasm` the init promise installs.
  await init();
  $('lintl-version').textContent = `compiler v${lintl_version()}`;
  compile_now();

  // Same-document permalink navigation (opening a share link while the tab
  // is already open) must re-read the hash too, not only the initial load.
  window.addEventListener('hashchange', () => {
    const next = decode_state();
    if (next === null) return;
    $('source').value = next.source;
    $('target').value = next.target;
    compile_now();
  });
}

main().catch((error) => {
  $('compiled-code').textContent = `failed to load the wasm compiler: ${error}`;
});

if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('./sw.js').catch(() => {
      /* offline caching is best-effort; the page itself still works */
    });
  });
}

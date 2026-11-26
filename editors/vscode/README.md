# Lintl for VS Code (T-16, grammar-only)

Syntax highlighting for `.lintl.php` — JSX inside PHP, exactly where the
language spec puts it (expression-start positions) — plus a problem matcher
that turns `lintl check` output into editor diagnostics.

**Acceptance (verbatim):** *JSX highlighted inside `.lintl.php`.* The
tokenization test below runs the grammar through the same engine VS Code
uses (`vscode-textmate` + `vscode-oniguruma`) and asserts the behavior: a
component and an HTML element at expression starts get markup scopes, a
`{...}` child keeps its PHP scope, `$a < $b` never becomes markup, and a
multiline element keeps nesting across lines.

## What ships

- `grammars/lintl.tmLanguage.json` — the grammar: PHP first (the editor's
  stock PHP grammar carries the language), JSX at expression starts, back-
  referenced close tags so children and nesting highlight.
- `language-configuration.json` — brackets, comments, auto-closing pairs.
- `package.json` — the `lintl` language bound to `*.lintl.php`, the
  `source.lintl.php` grammar with PHP embedded languages, and the
  `lintl-check` problem matcher.
- `test/tokenize.mjs` — the acceptance check (`npm test`).

## The problem matcher

Tasks run `lintl check` (exit 1 on error-severity diagnostics); attaching
the `lintl-check` matcher turns the rendered output —
`error[LNL0301]: message` followed by the `┌─ file:line:col` excerpt line —
into file-anchored editor problems:

```jsonc
// .vscode/tasks.json
{ "type": "shell", "command": "vendor/bin/lintl check src/", "problemMatcher": "$lintl-check" }
```

## Verify the grammar

```sh
npm install --no-audit --no-fund
npm test        # the 6 tokenization checks (CI's `editors` job runs these)
```

Packaging into a `.vsix` (vsce) is launch work; the grammar, language and
matcher here are the T-16 deliverable.

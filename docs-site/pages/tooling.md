# Tooling: incremental builds & the LSP workspace index

Two Phase 9 capabilities make Lintel pleasant at project scale: the build
cache that turns a warm `lintel build` into pure I/O, and the language
server's persistent workspace index that lets rename, references, symbols
and definition see the files you have not opened. Phase 11 extends the
index with an independent tree-sitter parse (the section below).

## Incremental builds (the build cache)

`lintel build` caches compiled PHP under `.lintel-cache/` in the workspace
root — the common ancestor of the build inputs and `--out`, so the cache
lands with the sources it describes from any invocation directory. One
entry per source **content** + **target** pair:

```
.lintel-cache/
  3f9c….php    # the compiled artifact (the string lowering's output)
  3f9c….json   # metadata sidecar: source path, target, options hash,
               # and the cold build's rendered warnings
  stats.json   # the last build's counters: compiled / cache_hits / bypassed
```

The key hashes the source **content** with the compile target — never the
mtime (a `git checkout` lies about mtimes) and never the path alone
(`--target=stream` and `--target=string` produce different artifacts for
the same source). An edited file or a changed target misses; everything
else is a hit, and a warm build compiles nothing: it reads the cached
artifact, replays the cold build's warnings verbatim (both `--format=text`
and `--format=json` shapes are stored) and writes the output — the §9.3
performance target "warm (cache hit): I/O-bound".

```sh
lintel build src/ --out build            # cold: compiles + stores
lintel build src/ --out build            # warm: all cache hits
lintel build src/ --out build --no-cache # bypasses lookup AND store (CI)
```

`--no-cache` exists for CI and debugging; `lintel check` and `lintel
analyze` never read the cache (they must always re-validate), and a
stdout build — the dev loader's compile lane — leaves no cache beside the
sources it compiles. Add `.lintel-cache/` to your `.gitignore` (the
repository's own does).

## LSP workspace index

The language server no longer limits rename, references, workspace
symbols and go-to-definition to the files you have open. On
`initialize` it walks the workspace root (from `workspaceFolders` or
`rootUri`), parses every `.lintel.php` file's component declarations, and
keeps them in an in-memory index backed by `.lintel-cache/lsp-index.json`
— a lightweight on-disk cache keyed by file mtime+size, so a fresh
session skips re-parsing unchanged files.

What that buys you:

- **rename** a component declared in a closed file — the edit set covers
  the declaration and every invocation, and the closed file's edits ride
  the `WorkspaceEdit` (computed against its disk bytes);
- **references** and **workspace/symbol** list closed-file declarations
  and usages;
- **definition** from a `<Component />` tag resolves to the declaring
  file's exact location even when that file is closed.

Stale entries evict themselves: every access re-validates the file's
mtime+size, and a client sync (`didOpen`/`didChange` full sync/`didSave`)
re-parses the synced text immediately — the editor's buffer is the truth
while the file is open, the disk re-asserts itself the moment it is not.
The walk skips dot-directories and the dependency directories
(`vendor`, `node_modules`).

## The tree-sitter lane (Phase 11)

Every declaration the index serves is a **merged** view: the scanner's
facts (the JSX-structure authority) unioned with an independent
tree-sitter parse of the same file — the upstream PHP grammar runs as a
second opinion on every workspace source, and its scan rides the same
`lsp-index.json` cache row. A component only one lane sees (a shape the
scanner misses, or — on malformed input — a declaration tree-sitter's
error recovery cannot see) still resolves, renames and lists references.
A file whose tree-sitter parse produces no usable declaration structure
falls back to the scanner alone, so broken input never blanks a file out
of the workspace. Markup regions stay the scanner's job: the PHP grammar
does not model JSX, so the lane contributes declaration structure while
lintel-core remains the authority on component invocations.

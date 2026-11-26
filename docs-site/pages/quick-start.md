# Quick start

A runnable Lintl page in five minutes. Everything below works from a git
checkout — signed release binaries, the Packagist `lintl/lintl` tag and
the `cargo install` shorthand land with the release-engineering card
(T-18); until then the compiler is built from source and the Composer
packages are wired through path repositories, which is exactly how CI and
the DX smoke test drive them.

## 1. Build the compiler

```sh
git clone https://github.com/elcoosp/lintl.git
cd lintl
cargo build --release -p lintl-cli
./target/release/lintl --version
```

The compiler is a single dependency-free binary; there is nothing else to
install. If you only want to *look* at Lintl first, the
[playground](/playground/) runs this exact compiler in your browser with no
install at all.

## 2. Scaffold a project

```sh
mkdir hello-lintl && cd hello-lintl
composer init --name you/hello-lintl --no-interaction
composer config repositories.lintl path ../lintl/php/lintl
composer config repositories.runtime path ../lintl/php/runtime
LINTL_BIN="$PWD/../lintl/target/release/lintl" composer require lintl/runtime
LINTL_BIN="$PWD/../lintl/target/release/lintl" composer require --dev lintl/lintl
```

Two packages, two jobs: `lintl/runtime` is the handful of escaping helpers
your production code ships with; `lintl/lintl` is the dev tooling — an
installer plugin that verifies and caches the binary at `vendor/bin/lintl`,
and a dev loader that recompiles a `.lintl.php` the moment its file size or
mtime changes, so **edit → refresh just works, no watcher process**. The
`LINTL_BIN` override tells the installer where the binary already lives;
without it the plugin prints a notice instead of inventing a download URL
(no live release URL exists before T-18).

## 3. Write a page

`hello.lintl.php`:

```php
<?php

declare(strict_types=1);

function Card(string $title, string $body): Html
{
    return <div class="card">
        <h2>{$title}</h2>
        <p>{$body}</p>
    </div>;
}

echo Card(title: 'It works', body: 'Compiled to plain PHP.');
```

## 4. Run it

```sh
vendor/bin/lintl build hello.lintl.php > hello.php
php -S localhost:8080
```

Open <http://localhost:8080/hello.php> — the page renders. In a real app
you never run step one by hand: `Lintl\Dev\Loader::boot()` (which
`lintl/lintl` sets up for you) compiles on refresh and caches by
mtime+size, so editing `hello.lintl.php` and hitting reload is the whole
workflow.

## 5. Learn the compiler's opinion

```sh
vendor/bin/lintl check src/     # diagnostics only, exit 1 on errors
vendor/bin/lintl stats src/     # component inventory per file
vendor/bin/lintl rules          # the built-in validation registry
vendor/bin/lintl init --demo 2  # the XSS gallery demo, runnable
```

`lintl init --demo N` scaffolds one of the five demos (component library,
XSS gallery, 10k-row table, HTMX todo, compile-error showcase) — each is
under 60 lines and executes under PHP with asserted output in the test
suite. When something goes wrong, the compiler's errors teach: every
diagnostic carries a code, a rendered source excerpt, a help line and a
docs link — all indexed under [the error index](/errors/).

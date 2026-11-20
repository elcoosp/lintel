# Quick start

A runnable Lintel page in five minutes. Everything below works from a git
checkout — signed release binaries, the Packagist `lintel/lintel` tag and
the `cargo install` shorthand land with the release-engineering card
(T-18); until then the compiler is built from source and the Composer
packages are wired through path repositories, which is exactly how CI and
the DX smoke test drive them.

## 1. Build the compiler

```sh
git clone https://github.com/elcoosp/lintel.git
cd lintel
cargo build --release -p lintel-cli
./target/release/lintel --version
```

The compiler is a single dependency-free binary; there is nothing else to
install. If you only want to *look* at Lintel first, the
[playground](/playground/) runs this exact compiler in your browser with no
install at all.

## 2. Scaffold a project

```sh
mkdir hello-lintel && cd hello-lintel
composer init --name you/hello-lintel --no-interaction
composer config repositories.lintel path ../lintel/php/lintel
composer config repositories.runtime path ../lintel/php/runtime
LINTEL_BIN="$PWD/../lintel/target/release/lintel" composer require lintel/runtime
LINTEL_BIN="$PWD/../lintel/target/release/lintel" composer require --dev lintel/lintel
```

Two packages, two jobs: `lintel/runtime` is the handful of escaping helpers
your production code ships with; `lintel/lintel` is the dev tooling — an
installer plugin that verifies and caches the binary at `vendor/bin/lintel`,
and a dev loader that recompiles a `.lintel.php` the moment its file size or
mtime changes, so **edit → refresh just works, no watcher process**. The
`LINTEL_BIN` override tells the installer where the binary already lives;
without it the plugin prints a notice instead of inventing a download URL
(no live release URL exists before T-18).

## 3. Write a page

`hello.lintel.php`:

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
vendor/bin/lintel build hello.lintel.php > hello.php
php -S localhost:8080
```

Open <http://localhost:8080/hello.php> — the page renders. In a real app
you never run step one by hand: `Lintel\Dev\Loader::boot()` (which
`lintel/lintel` sets up for you) compiles on refresh and caches by
mtime+size, so editing `hello.lintel.php` and hitting reload is the whole
workflow.

## 5. Learn the compiler's opinion

```sh
vendor/bin/lintel check src/     # diagnostics only, exit 1 on errors
vendor/bin/lintel stats src/     # component inventory per file
vendor/bin/lintel rules          # the built-in validation registry
vendor/bin/lintel init --demo 2  # the XSS gallery demo, runnable
```

`lintel init --demo N` scaffolds one of the five demos (component library,
XSS gallery, 10k-row table, HTMX todo, compile-error showcase) — each is
under 60 lines and executes under PHP with asserted output in the test
suite. When something goes wrong, the compiler's errors teach: every
diagnostic carries a code, a rendered source excerpt, a help line and a
docs link — all indexed under [the error index](/errors/).

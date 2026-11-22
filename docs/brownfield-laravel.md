# Adding Lintel to an existing Laravel app in under 5 minutes

The most common real entry point is not a greenfield project — it is an
existing Laravel application that wants Lintel's escape-by-default JSX for
*new* components without rewriting the Blade templates that already work.
This guide wires Lintel into a running Laravel app in under five minutes,
side-by-side with Blade, with no disruption to the existing templates.

## Prerequisites

- PHP 8.6+ (Laravel 13+; this guide targets Laravel 13)
- Composer
- A prebuilt `lintel` binary (the installer plugin fetches it on
  `composer require`; if you are on a fresh checkout before the v0.1.0
  tag, build from source: `cargo build --release -p lintel-cli` and set
  `LINTEL_BIN` to `target/release/lintel`)

## Step 1 — require the Blade bridge (1 min)

```bash
composer require lintel/blade
```

The `lintel/blade` package brings in `lintel/lintel` (the installer plugin
that fetches the prebuilt binary) and `lintel/runtime` (the escaping
helpers). On `composer install`, the plugin caches `vendor/bin/lintel`.

If you are on a pre-release checkout, point the path repositories at your
local clone first:

```bash
composer config repositories.lintel path /path/to/lintel/php/lintel
composer config repositories.runtime path /path/to/lintel/php/runtime
composer config repositories.blade path /path/to/lintel/php/blade
composer require lintel/blade
```

## Step 2 — register the service provider (1 min)

Add the `LintelServiceProvider` to `config/app.php` (or
`bootstrap/providers.php` on Laravel 11+):

```php
return [
    // ...
    'providers' => [
        // ...
        Lintel\Blade\LintelServiceProvider::class,
    ],
];
```

The provider registers the `@lintel` Blade directive and the
`lintel:build` artisan command. No existing Blade behavior changes.

## Step 3 — declare your component sources (30 sec)

Create `lintel.json` at the project root:

```json
{
    "functions": ["resources/views/components/*.lintel.php"]
}
```

The glob tells the compiler (and the dev loader) where your `.lintel.php`
sources live. Existing `.blade.php` files are untouched.

## Step 4 — write your first Lintel component (1 min)

```php
<?php
// resources/views/components/Badge.lintel.php
namespace App\View\Components;

use Lintel\Html;

function Badge(string $label, bool $verified = false): Html
{
    return <span class="badge">
        {$label}
        {$verified && <i class="icon-check" />}
    </span>;
}
```

The component is a plain PHP function — no class, no inheritance, no
attribute discovery. The `Html` return type is the value the string
target emits; typed `string` parameters escape directly through
`htmlspecialchars` (escape elision, see [escape-elision.md](../escape-elision.md)).

## Step 5 — render it from Blade (30 sec)

Use the `@lintel` directive in any existing `.blade.php` template:

```blade
<!-- resources/views/welcome.blade.php -->
@extends('layouts.app')

@section('content')
    <h1>Welcome</h1>
    @lintel('Badge', ['label' => $user->name, 'verified' => $user->verified])
@endsection
```

The directive resolves the component, spreads the props as PHP named
arguments, and prints the result through the runtime's `c()` — so a
Blade-embedded Lintel component is not an escape bypass. Every dynamic
value in the Lintel component is escaped by Lintel's helpers; every
dynamic value in the surrounding Blade template is escaped by Blade's
`{{ }}`.

## Step 6 — build for production (30 sec)

```bash
php artisan lintel:build
```

The command compiles every `.lintel.php` source in the manifest's
`functions` glob into `bootstrap/cache/lintel/`, writes the `prod`
artifacts map into `lintel.json`, and regenerates the Composer autoloader
so the function components autoload on the first request (no dev loader,
no manual require — the T-31 prod lane). The dev loader (`Loader::boot`)
is for development only; it recompiles changed sources on demand with no
watcher.

## Verification

Visit a page that renders the `@lintel('Badge', ...)` directive. The
rendered HTML carries the escaped label and the conditional icon:

```html
<span class="badge">Jane Doe<i class="icon-check"></i></span>
```

Edit `Badge.lintel.php`, refresh, and the dev loader recompiles (the
cache key is mtime+size). For production, `php artisan lintel:build`
pre-compiles everything.

## What does NOT change

- Existing `.blade.php` templates render exactly as before — Blade is
  untouched.
- The `{{ }}` escaping in Blade is independent of Lintel's `c()`; the two
  escape-by-default systems coexist without interference.
- Routes, middleware, Eloquent models, and the rest of the Laravel stack
  are unaware of Lintel.

## Troubleshooting

- **`Class Lintel\Blade\LintelServiceProvider not found`** — run
  `composer dump-autoload` after registering the provider.
- **`lintel:build` reports no sources** — check `lintel.json`'s `functions`
  glob matches your `.lintel.php` files (`ls resources/views/components/*.lintel.php`).
- **The dev loader serves stale output** — the cache key is mtime+size; if
  your editor writes with `O_TRUNC` and the same size, `touch -d '1 hour ago'`
  the source to force a recompile, or `rm -rf bootstrap/cache/lintel/`.

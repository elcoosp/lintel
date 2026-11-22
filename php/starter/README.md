# The Lintel starter (M2 release-readiness)

A zero-Rust first-run project: `composer create-project lintel/starter my-app`
&& `cd my-app && php -S localhost:8000` renders a working page with **no Rust
toolchain installed** — the Lintel installer plugin fetches the prebuilt
binary on `composer install`.

## What's in here

```
php/starter/
├── composer.json       # the project template create-project consumes
├── lintel.json         # the manifest: { "functions": ["components/*.lintel.php"] }
├── components/
│   ├── Card.lintel.php  # a typed component: child + URL attr + bool attr
│   └── Page.lintel.php  # composition + a fragment + a list of children
└── public/
    └── index.php        # the entrypoint: dev loader + render the Page
```

## The local smoke (the proof the starter works end-to-end)

The starter ships with a smoke script (`scripts/starter-smoke.sh`) that
proves the zero-Rust path against the locally built binary. The path
repositories in `composer.json` point at the local checkout's
`php/lintel` and `php/runtime` packages (the same pattern `scripts/dx-smoke.sh`
uses), so the smoke runs the real installer plugin with `LINTEL_BIN` set to
the release build — exactly the lane a consumer hits, minus the
published-package download (which is DEFERRED-TO-MAINTAINER: the
`lintel/lintel` and `lintel/runtime` packages must be on Packagist first,
then `composer create-project lintel/starter my-app` resolves them from
the registry instead of the path repos).

Run it:

```bash
cargo build --release -p lintel-cli
LINTEL_BIN=$PWD/target/release/lintel bash scripts/starter-smoke.sh
```

## The published-package path (the maintainer's remaining step)

Once `lintel/lintel` and `lintel/runtime` are published to Packagist (the
release.yml `publish` job does this on tag), the starter's path
repositories are removed and `composer create-project lintel/starter
my-app` resolves everything from the registry — the installer plugin's
`fetchBinary()` downloads the prebuilt `lintel` binary from the GitHub
release (the `LINTEL_RELEASE_ROOT` default points at the release assets),
verifies its SHA-256 against `SHA256SUMS`, and caches it at
`vendor/bin/lintel`. The consumer never installs Rust.

The exact maintainer step: after the v0.1.0 tag, publish `lintel/lintel`
and `lintel/runtime` to Packagist (release.yml does this), then either
publish `lintel/starter` as a Packagist package OR point the
`create-project` command at `elcoosp/lintel:php/starter` (the monorepo
subdirectory). The path repositories in `composer.json` are removed in
that step (they exist only so the local smoke runs without a published
package).

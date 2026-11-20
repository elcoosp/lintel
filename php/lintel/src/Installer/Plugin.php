<?php

declare(strict_types=1);

namespace Lintel\Installer;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;

/**
 * The §12 composer plugin, two jobs on every autoload dump:
 *
 * 1. `post-autoload-dump` — make sure `<vendor>/bin/lintel` exists, from
 *    the LINTEL_BIN override or from the pinned `extra.lintel` source +
 *    SHA-256 (BinaryInstaller::resolve owns the precedence, the
 *    verification and the no-invented-URL rule). Prod never needs the
 *    binary: it runs the compiled output plus lintel/runtime.
 * 2. `pre-autoload-dump` (T-31) — read the project's `lintel.json` and
 *    turn its `prod.artifacts` section (emitted by `lintel build
 *    --manifest`, a source → artifact map) into a generated `files`
 *    autoload entry, so function-only components — which PSR-4 can never
 *    autoload — are defined in prod without any manual require. The shim
 *    records every artifact's source in `$GLOBALS['lintelFromSource']`,
 *    which the dev loader (§6.10) consults to skip sources the shim
 *    already loaded — the two lanes never redeclare a function in one
 *    process. The injection modifies the root package's autoload for this
 *    dump only (in-memory, `composer.json` stays untouched) and is
 *    idempotent across repeated dumps.
 */
final class Plugin implements PluginInterface, EventSubscriberInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        // State-free: every dump event re-resolves from the environment and
        // composer.json, so LINTEL_BIN changes apply on the next dump.
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
        // Nothing to unwind: the plugin holds no state (see activate()).
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
        // vendor/bin/lintel is a cache, not a plugin artifact (§12) — the
        // next autoload dump re-installs it, so nothing is cleaned up here.
    }

    /**
     * @return array<string, string>
     */
    public static function getSubscribedEvents(): array
    {
        return [
            ScriptEvents::POST_AUTOLOAD_DUMP => 'installBinary',
            ScriptEvents::PRE_AUTOLOAD_DUMP => 'injectFunctionAutoload',
        ];
    }

    public function installBinary(Event $event): void
    {
        $composer = $event->getComposer();
        $vendorDir = $composer->getConfig()->get('vendor-dir');
        BinaryInstaller::resolve(
            $composer->getPackage()->getExtra(),
            (\is_string($vendorDir) && $vendorDir !== '' ? $vendorDir : 'vendor') . '/bin',
            $event->getIo(),
        );
    }

    /**
     * The T-31 prod lane: `lintel.json`'s `prod.artifacts` (the source →
     * artifact pairs a `lintel build --manifest --out DIR` run wrote)
     * become a generated `<vendor>/lintel/functions.php` required through
     * the root package's `files` autoload — Composer resolves those
     * entries against the project root, exactly where the manifest's
     * relative paths point. Each required artifact's source is recorded in
     * `$GLOBALS['lintelFromSource']` for the dev loader's skip check.
     *
     * Lenient by design: composer install must never fail over the
     * manifest. A missing manifest or a missing/empty `prod.artifacts`
     * section is a silent no-op; a malformed one is an IO notice.
     */
    public function injectFunctionAutoload(Event $event): void
    {
        $composer = $event->getComposer();
        $io = $event->getIo();
        $config = $composer->getConfig();
        $vendorDir = $config->get('vendor-dir');
        $vendorDir = \is_string($vendorDir) && $vendorDir !== '' ? $vendorDir : 'vendor';
        $root = self::projectRoot($vendorDir);
        $manifest = $root . '/lintel.json';
        if (!\is_file($manifest)) {
            return;
        }
        $decoded = \json_decode((string) \file_get_contents($manifest), true);
        if (!\is_array($decoded)) {
            $io->writeError(
                'lintel: lintel.json at ' . $root
                . ' is not a valid JSON object; skipping the prod function autoload'
            );

            return;
        }
        // `prod` is a known manifest section (T-31's `lintel build
        // --manifest` shape) — validated like the top-level object, so
        // the `artifacts` lookup indexes a real array.
        $prod = $decoded['prod'] ?? null;
        if (!\is_array($prod)) {
            $io->writeError(
                'lintel: lintel.json at ' . $root
                . ': "prod" must be an object; skipping the prod function autoload'
            );

            return;
        }
        $pairs = $prod['artifacts'] ?? null;
        if ($pairs === null || $pairs === []) {
            // No (or empty) prod section — a dev-only project, or a stdout
            // build: nothing to inject is not an error.
            return;
        }
        if (!\is_array($pairs)) {
            $io->writeError(
                'lintel: lintel.json at ' . $root
                . ': "prod.artifacts" must be a source → artifact map; skipping the prod function autoload'
            );

            return;
        }
        foreach ($pairs as $source => $artifact) {
            if (!\is_string($source) || $source === '' || !\is_string($artifact) || $artifact === '') {
                $io->writeError(
                    'lintel: lintel.json at ' . $root
                    . ': "prod.artifacts" must map non-empty source paths to non-empty artifact paths; '
                    . 'skipping the prod function autoload'
                );

                return;
            }
        }
        \ksort($pairs);
        $vendorLintel = $vendorDir . '/lintel';
        if (!\is_dir($vendorLintel) && !@\mkdir($vendorLintel, 0777, true) && !\is_dir($vendorLintel)) {
            $io->writeError('lintel: cannot create ' . $vendorLintel . '; skipping the prod function autoload');

            return;
        }
        \file_put_contents($vendorLintel . '/functions.php', self::generatedFile($pairs));
        $entry = self::rootRelative($vendorLintel . '/functions.php', $root);
        if ($entry === null) {
            $io->writeError(
                'lintel: vendor dir is outside the project root; skipping the prod function autoload'
            );

            return;
        }
        $package = $composer->getPackage();
        // Composer's autoloader contract types `getAutoload()` as the
        // manifest's autoload map (`files?: list<string>` among them) —
        // no re-narrowing needed.
        $autoload = $package->getAutoload();
        $existing = $autoload['files'] ?? [];
        if (!\in_array($entry, $existing, true)) {
            $existing[] = $entry;
        }
        $autoload['files'] = $existing;
        $package->setAutoload($autoload);
        $io->write('lintel: ' . \count($existing) . ' function-component artifact(s) injected into the files autoload');
    }

    /**
     * The project root a `<vendor>` dir lives in — Composer resolves the
     * root package's `files` autoload entries against it (its own
     * `autoload_files.php` computes `$baseDir = dirname($vendorDir)`).
     */
    private static function projectRoot(string $vendorDir): string
    {
        $real = \realpath($vendorDir);
        if ($real !== false && \is_dir($real)) {
            return \dirname($real);
        }

        return \dirname($vendorDir);
    }

    /**
     * The `files` entry Composer stores: relative to the project root (an
     * absolute entry would be spliced behind `$baseDir . '/'` and break).
     * An already-relative vendor dir stays as-is; `null` means the vendor
     * tree sits outside the root and the entry is inexpressible.
     */
    private static function rootRelative(string $path, string $root): ?string
    {
        if (!\str_starts_with($path, '/')) {
            return $path;
        }
        if (\str_starts_with($path, $root . '/')) {
            return \substr($path, \strlen($root) + 1);
        }

        return null;
    }

    /**
     * The generated require-shim: each artifact is required once against
     * `dirname(__DIR__, 2)` — the same `$baseDir = dirname($vendorDir)`
     * convention Composer's own `autoload_files.php` uses, so the shim
     * works from any vendor location and survives relocation — and every
     * source → artifact pair records the source in the
     * `$GLOBALS['lintelFromSource']` registry the dev loader consults.
     *
     * @param array<string, string> $pairs manifest-relative source → artifact
     */
    private static function generatedFile(array $pairs): string
    {
        $out = <<<'HEADER'
<?php

// @generated by lintel/lintel from lintel.json (architecture doc §6.10, T-31) — do not edit.
// Rebuild with `lintel build --manifest`; the Composer `files` autoload
// requires every compiled function-component artifact, so prod needs no
// manual require. Required artifacts record their source, so the dev
// loader never redeclares what this shim already defined.

declare(strict_types=1);

$lintelBase = \dirname(__DIR__, 2);
$lintelRequired = $GLOBALS['lintelProdArtifacts'] ?? [];
$lintelFromSource = [];

foreach ([
HEADER;
        $out .= "\n";
        foreach ($pairs as $source => $artifact) {
            $out .= '    \'' . \str_replace('\'', '\\\'', $source) . '\' => '
                . '\'' . \str_replace('\'', '\\\'', $artifact) . '\',' . "\n";
        }
        $out .= "] as \$lintelSource => \$lintelArtifact) {\n";
        $out .= "    if (!isset(\$lintelRequired[\$lintelArtifact])) {\n";
        $out .= "        \$lintelRequired[\$lintelArtifact] = true;\n";
        $out .= "        require \\str_starts_with(\$lintelArtifact, '/')\n";
        $out .= "            ? \$lintelArtifact\n";
        $out .= "            : \$lintelBase . '/' . \$lintelArtifact;\n";
        $out .= "    }\n";
        $out .= "    \$lintelFromSource[\$lintelSource] = true;\n";
        $out .= "}\n";
        $out .= "\n\$GLOBALS['lintelProdArtifacts'] = \$lintelRequired;\n";
        $out .= "\$GLOBALS['lintelFromSource'] = (\$GLOBALS['lintelFromSource'] ?? []) + \$lintelFromSource;\n";

        return $out;
    }
}

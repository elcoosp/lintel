<?php

declare(strict_types=1);

namespace Lintel\Installer;

use Composer\IO\IOInterface;

/**
 * The §12 download+verify+cache path: the plugin fetches the lintel binary,
 * verifies its SHA-256 (§12: "pinned versions, checksum file ... no network
 * at runtime"), and caches it executable at `<vendor>/bin/lintel` —
 * atomically, or not at all. No live release URL is invented here: the real
 * ones arrive with T-18, so a consumer pins `extra.lintel` or uses the
 * LINTEL_BIN override.
 */
final class BinaryInstaller
{
    /**
     * Download $url and cache it at $dest only when its SHA-256 equals
     * $expectedSha256.
     *
     * @throws InstallerException on download or checksum failure
     */
    public static function downloadVerified(string $url, string $expectedSha256, string $dest): string
    {
        $data = @\file_get_contents($url, false, \stream_context_create(['http' => ['timeout' => 60]]));
        if ($data === false) {
            $error = \error_get_last()['message'] ?? 'download failed';
            throw new InstallerException("download failed for {$url}: {$error}");
        }
        $actual = \hash('sha256', $data);
        if (!\hash_equals($expectedSha256, $actual)) {
            throw new InstallerException(
                "SHA-256 mismatch for {$url}: expected {$expectedSha256}, got {$actual} — the binary is refused",
            );
        }

        return self::writeExecutable($dest, $data);
    }

    /**
     * Cache a locally provided binary (the LINTEL_BIN override, §8.1) at
     * $dest without touching the network.
     *
     * @throws InstallerException when the override points at nothing
     */
    public static function installLocal(string $sourcePath, string $dest): string
    {
        if (!\is_file($sourcePath)) {
            throw new InstallerException("LINTEL_BIN points at a missing binary: {$sourcePath}");
        }
        $data = (string) \file_get_contents($sourcePath);

        return self::writeExecutable($dest, $data);
    }

    /**
     * Resolve and install the lintel binary into $vendorBinDir/lintel (§12).
     * Precedence: the LINTEL_BIN override (copied, no network) → the
     * `extra.lintel` source + SHA-256 (downloaded, verified, cached) → an
     * install-time notice and nothing installed (no release URLs exist
     * before T-18, so none are invented).
     *
     * @param array<mixed> $extra the root package's `extra` section (the
     *                            composer.json object; `lintel` is the key
     *                            this resolver reads)
     *
     * @return string|null the installed path, or null when unconfigured
     *
     * @throws InstallerException on a bad override, a checksum mismatch or a
     *                            `extra.lintel.url` without a `sha256`
     */
    public static function resolve(array $extra, string $vendorBinDir, IOInterface $io): ?string
    {
        $dest = \rtrim($vendorBinDir, '/') . '/lintel';
        $override = \getenv('LINTEL_BIN');
        if (\is_string($override) && $override !== '') {
            $io->write("lintel installer: LINTEL_BIN override — caching {$override} at {$dest} (no network)");

            return self::installLocal($override, $dest);
        }
        $pin = $extra['lintel'] ?? null;
        if (!\is_array($pin)) {
            $io->write(
                'lintel installer: no binary source configured — set LINTEL_BIN, or pin extra.lintel'
                . ' {url, sha256} in composer.json (no live release URL exists before T-18; nothing downloaded)',
            );

            return null;
        }
        $url = $pin['url'] ?? null;
        $sha = $pin['sha256'] ?? null;
        if (!\is_string($url) || $url === '') {
            throw new InstallerException('extra.lintel.url must be a non-empty string');
        }
        if (!\is_string($sha) || $sha === '') {
            throw new InstallerException(
                'extra.lintel.sha256 must be a non-empty SHA-256 — an unverified binary never installs (§12)',
            );
        }
        $io->write(
            "lintel installer: downloading {$url} and verifying its SHA-256"
            . " before caching it at {$dest}",
        );

        return self::downloadVerified($url, $sha, $dest);
    }

    /**
     * Atomic, executable cache write: bytes land at `<dest>.tmp`, get
     * chmod'ed 0755, and rename over $dest — a failed install never leaves
     * a half-written binary or stray temp file behind.
     *
     * @throws InstallerException when the destination cannot be written
     */
    private static function writeExecutable(string $dest, string $data): string
    {
        $dir = \dirname($dest);
        if (!\is_dir($dir)) {
            @\mkdir($dir, 0777, true);
        }
        \file_put_contents($dest . '.tmp', $data);
        if (!@\chmod($dest . '.tmp', 0755) || !@\rename($dest . '.tmp', $dest)) {
            throw new InstallerException("cannot cache the lintel binary at {$dest}");
        }

        return $dest;
    }
}

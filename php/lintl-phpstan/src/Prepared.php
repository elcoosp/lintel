<?php

declare(strict_types=1);

namespace Lintl\PhpStan;

/**
 * What {@see Bridge::prepare()} produced for one source file: the paths a
 * phpstan.neon consumes. `artifact` is empty when the source failed to
 * compile (the diagnostics still surfaced through the metadata sidecar);
 * `stub` is empty when the source declares no components.
 */
final class Prepared
{
    public function __construct(
        /** The source `.lintl.php` absolute path. */
        public readonly string $source,
        /** The compiled artifact path, or '' when the source has errors. */
        public readonly string $artifact,
        /** The metadata sidecar path the diagnostics rule reads. */
        public readonly string $metadata,
        /** The signature stub path for `stubFiles`, or '' when none apply. */
        public readonly string $stub,
    ) {}
}

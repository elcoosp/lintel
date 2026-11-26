<?php

declare(strict_types=1);

namespace Lintl\PhpStan;

use PhpParser\Node;
use PHPStan\Analyser\Scope;
use PHPStan\Node\FileNode;
use PHPStan\Rules\Rule;
use PHPStan\Rules\RuleErrorBuilder;

/**
 * The PHPStan side of the bridge (architecture doc §17.1 Q1): for every
 * file PHPStan analyses, this rule consults the metadata sidecar the
 * {@see Bridge} wrote next to compiled Lintl artifacts and turns each
 * recorded compiler diagnostic into a PHPStan error — reported against
 * the SOURCE `.lintl.php` path and line (the compiler's JSON positions,
 * `docs/diagnostics-json.md`, are renderer-parity source positions), with
 * the stable `LNLxxxx` code as the error identifier (`lintl.LNLxxxx`)
 * and the compiler's `help:` advice as a tip.
 *
 * Every ordinary PHP file (no sidecar) passes through untouched, so the
 * rule costs nothing outside compiled artifacts. Severity rides the code
 * and message: PHPStan rule errors carry no per-error severity, and the
 * LNL classification is part of the diagnostic itself.
 *
 * @implements Rule<FileNode>
 */
final class LintlDiagnosticsRule implements Rule
{
    public function getNodeType(): string
    {
        return FileNode::class;
    }

    public function processNode(Node $node, Scope $scope): array
    {
        $metadata = Bridge::metadataFor($scope->getFile());
        if ($metadata === null) {
            return [];
        }

        $errors = [];
        // The sidecar's diagnostics are decoded into the validated shape
        // (`Bridge::decodeDiagnostic`) — every field the rule reports is
        // already the compiler's own value or the documented fallback.
        foreach ($metadata['diagnostics'] as $diagnostic) {
            $builder = RuleErrorBuilder::message($diagnostic['message'])
                ->identifier('lintl.'.$diagnostic['code'])
                ->file($metadata['source'])
                ->line(max(1, $diagnostic['line']));
            if ($diagnostic['help'] !== null && $diagnostic['help'] !== '') {
                $builder->tip($diagnostic['help']);
            }
            $errors[] = $builder->build();
        }

        return $errors;
    }
}

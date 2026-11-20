<?php

declare(strict_types=1);

namespace Lintel\PhpStan;

/**
 * The PHPStan side of the §7.3-Y2 analyze pass (architecture doc §7.3-Y2,
 * `docs/type-elision.md`): for every `\Lintel\c()` call in an analysed
 * artifact, this rule resolves the argument's type through PHPStan's
 * analyser (the signature stubs the {@see Bridge} wrote carry the scanner's
 * proven facts, the consumer's own types narrow the rest) and REPORTS the
 * provably-safe ones — one "error" per call whose message is a JSON
 * payload the analyze runner turns into the compiler's types file.
 *
 * The payload keys are exactly what the compiler matches against:
 * `line` — the argument's start line (the artifact is line-preserved, so
 * it is the source line too, §5.2); `expr` — the argument's exact text
 * sliced from the artifact (the lowering emits the trimmed source text
 * verbatim inside `c(...)`); `type` — the proven kind; `value` — for a
 * `literal`, the constant's value. Types that are NOT elidable still
 * report (`bool`, plain `string`, …) so the report is a real type surface;
 * the compiler's allowlist decides what folds.
 *
 * This rule ships in the analyze ruleset the runner writes — never in
 * `extension.neon` — so a consumer's own PHPStan run sees no type-report
 * errors.
 *
 * @implements \PHPStan\Rules\Rule<\PhpParser\Node\Expr\CallLike>
 */
final class LintelTypesCollector implements \PHPStan\Rules\Rule
{
    /** Artifact contents per path — the argument text is sliced from them. */
    /** @var array<string, string> */
    private static array $contents = [];

    public function getNodeType(): string
    {
        return \PhpParser\Node\Expr\CallLike::class;
    }

    public function processNode(\PhpParser\Node $node, \PHPStan\Analyser\Scope $scope): array
    {
        if (! $node instanceof \PhpParser\Node\Expr\FuncCall) {
            // The template already binds this rule to `CallLike` nodes;
            // this narrows the call-like family to plain function calls.
            return [];
        }
        if ($node->isFirstClassCallable() || ! $node->name instanceof \PhpParser\Node\Name) {
            return [];
        }

        if ($node->name->toString() !== 'Lintel\c') {
            return [];
        }
        $args = $node->getArgs();
        if (count($args) !== 1 || $args[0]->unpack) {
            return [];
        }
        $kind = self::kind($scope->getType($args[0]->value));
        $contents = self::fileContents($scope->getFile());
        if ($contents === '') {
            return [];
        }
        $argument = $args[0]->value;
        $start = $argument->getStartFilePos();
        $end = $argument->getEndFilePos();
        if ($start < 0 || $end < $start) {
            return [];
        }
        // The guards above bound the slice inside the file and give it at
        // least one byte, so `substr` always yields the argument's text.
        $expr = (string) substr($contents, $start, $end - $start + 1);
        if ($expr === '') {
            return [];
        }
        $payload = [
            'line' => $argument->getStartLine(),
            'expr' => $expr,
            'type' => $kind['type'],
        ];
        if (isset($kind['value'])) {
            $payload['value'] = $kind['value'];
        }

        return [
            \PHPStan\Rules\RuleErrorBuilder::message((string) json_encode($payload, JSON_THROW_ON_ERROR))
                ->identifier('lintel.types')
                ->build(),
        ];
    }

    /**
     * The elision classification of one PHPStan type: the allowlist kinds
     * with their report strings, the non-elidable kinds as themselves, and
     * `mixed` for every other resolved shape — the report names every
     * `c()` argument, and the compiler's allowlist decides what folds.
     * `bool` is reported but never elidable — `c()` renders every bool as
     * `''` while a raw bool interpolates as `'1'`, so no fold preserves
     * the output (`docs/type-elision.md` records the decision).
     *
     * @return array{type: string, value?: string}
     */
    private static function kind(\PHPStan\Type\Type $type): array
    {
        if ($type->isInteger()->yes()) {
            return ['type' => 'int'];
        }
        if ($type->isFloat()->yes()) {
            return ['type' => 'float'];
        }
        if ($type->isBoolean()->yes()) {
            return ['type' => 'bool'];
        }
        if ($type->isLiteralString()->yes()) {
            $values = $type->getConstantScalarValues();
            // A single constant value folds to its escaped literal; a
            // multi-value literal union has no compile-time value to
            // inline, so it reports as plain `string`.
            if (count($values) === 1 && is_string($values[0]) && preg_match('//u', $values[0]) === 1) {
                return ['type' => 'literal', 'value' => $values[0]];
            }
            return ['type' => 'string'];
        }
        if ($type->isNumericString()->yes()) {
            return ['type' => 'numeric-string'];
        }
        if ($type->isString()->yes()) {
            return ['type' => 'string'];
        }
        // Every other resolved shape (mixed, arrays, objects, unknown
        // unions) reports under its conservative bucket: the report is a
        // real type surface, and the compiler keeps the helper for all of
        // them.
        return ['type' => 'mixed'];
    }

    private static function fileContents(string $path): string
    {
        if (! array_key_exists($path, self::$contents)) {
            self::$contents[$path] = is_file($path) ? (string) file_get_contents($path) : '';
        }
        return self::$contents[$path];
    }
}

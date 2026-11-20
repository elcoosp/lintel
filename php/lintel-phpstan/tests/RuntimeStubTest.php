<?php

declare(strict_types=1);

namespace Lintel\Tests;

use PHPUnit\Framework\TestCase;

use function array_map;
use function array_slice;
use function class_exists;
use function dirname;
use function file_exists;
use function file_get_contents;
use function preg_match_all;
use function sprintf;

/**
 * The shipped `runtime.stub` mirrors the real runtime's public surface
 * (`php/runtime/src/functions.php`, §6.8): every helper function and every
 * `Html` member the compiled artifacts may call must be declared in the
 * stub with the same parameter names and return type — a runtime helper
 * added without shipping it here fails this test, so the PHPStan bridge
 * can never go stale against the runtime.
 */
final class RuntimeStubTest extends TestCase
{
    private const RUNTIME = __DIR__ . '/../../runtime/src/functions.php';

    private const STUB = __DIR__ . '/../resources/runtime.stub';

    /** @var array<string, array<int, array{name: string, type: string}>> function → ordered params */
    private array $stubFunctions = [];

    /** @var array<string, array<int, array{name: string, type: string}>> method → ordered params */
    private array $stubMethods = [];

    public static function setUpBeforeClass(): void
    {
        self::assertTrue(file_exists(self::RUNTIME), 'the runtime package sits beside the bridge (monorepo layout)');
        self::assertTrue(file_exists(self::STUB), 'the runtime stub ships with the package');
        require_once self::RUNTIME;
    }

    protected function setUp(): void
    {
        $stub = (string) file_get_contents(self::STUB);
        preg_match_all(
            '/^function ([a-zA-Z_]+)\(([^)]*)\)\s*:\s*(\??[\w\\\\]+)\s*\{/m',
            $stub,
            $functions,
            PREG_SET_ORDER,
        );
        foreach ($functions as $match) {
            $this->stubFunctions['Lintel\\' . $match[1]] = self::parseParams($match[2]);
        }
        preg_match_all(
            '/^    (?:public |private |protected |static |readonly )*function ([a-zA-Z_]+)\(([^)]*)\)/m',
            $stub,
            $methods,
            PREG_SET_ORDER,
        );
        foreach ($methods as $match) {
            $this->stubMethods[$match[1]] = self::parseParams($match[2]);
        }
    }

    public function testEveryRuntimeHelperIsDeclaredInTheStub(): void
    {
        $declared = self::runtimeFunctions();
        foreach ($declared as $name => $params) {
            self::assertArrayHasKey(
                $name,
                $this->stubFunctions,
                sprintf('runtime helper %s() is not declared in resources/runtime.stub', $name),
            );
            self::assertSame(
                $params,
                $this->stubFunctions[$name],
                sprintf('the stub signature of %s() drifted from the runtime', $name),
            );
        }
        self::assertCount(count($declared), $this->stubFunctions, 'the stub declares only real helpers');
    }

    public function testTheHtmlClassSurfaceMatches(): void
    {
        self::assertTrue(class_exists('Lintel\\Html'));
        $reflection = new \ReflectionClass('Lintel\\Html');
        foreach ($reflection->getMethods() as $method) {
            self::assertArrayHasKey(
                $method->getName(),
                $this->stubMethods,
                sprintf('Html::%s() is not declared in resources/runtime.stub', $method->getName()),
            );
            self::assertSame(
                array_map(
                    static fn (\ReflectionParameter $parameter): array => [
                        'name' => '$' . $parameter->getName(),
                        'type' => self::typeString($parameter->getType()),
                    ],
                    $method->getParameters(),
                ),
                $this->stubMethods[$method->getName()],
                sprintf('the stub signature of Html::%s() drifted', $method->getName()),
            );
        }
    }

    /**
     * The runtime's helper functions with their ordered `$name`/type pairs.
     *
     * @return array<string, array<int, array{name: string, type: string}>>
     */
    private static function runtimeFunctions(): array
    {
        $names = ['e', 'c', 'a', 'u', 'srcset', 'cls', 'sty', 'attrs'];
        $declared = [];
        foreach ($names as $name) {
            $reflection = new \ReflectionFunction('Lintel\\' . $name);
            $declared['Lintel\\' . $name] = array_map(
                static fn (\ReflectionParameter $parameter): array => [
                    'name' => '$' . $parameter->getName(),
                    'type' => self::typeString($parameter->getType()),
                ],
                $reflection->getParameters(),
            );
        }
        return $declared;
    }

    /** @return array<int, array{name: string, type: string}> */
    private static function parseParams(string $paramsSrc): array
    {
        if (trim($paramsSrc) === '') {
            return [];
        }
        $params = [];
        foreach (explode(',', $paramsSrc) as $segment) {
            $segment = trim($segment);
            // `type $name = default` / `...$name` — the name is the first
            // `$identifier`; the type is the text before it.
            if (preg_match('/^(.*?)\$\w+/', $segment, $match) === 1) {
                $type = trim($match[1], ' &');
                // Promotion modifiers ride the declaration, not the type.
                foreach (['public', 'private', 'protected', 'readonly'] as $modifier) {
                    $type = preg_replace('/^' . $modifier . '\s+/i', '', $type) ?? $type;
                }
                $params[] = [
                    'name' => self::paramName($segment),
                    'type' => self::normalize($type),
                ];
            }
        }
        return $params;
    }

    /** The first `$identifier` in a parameter source, `''` when none. */
    private static function paramName(string $segment): string
    {
        if (preg_match('/\$(\w+)/', $segment, $nameMatch) !== 1) {
            return '';
        }
        return '$' . $nameMatch[1];
    }

    private static function typeString(?\ReflectionType $type): string
    {
        if ($type === null) {
            return '';
        }
        return self::normalize((string) $type);
    }

    /** Normalizes a type for structural comparison (stubs spell `\Lintel\…`). */
    private static function normalize(string $type): string
    {
        $type = str_replace(['?'], '', $type);
        $type = ltrim($type, '\\');
        // The runtime spells namespaces with a leading backslash in some
        // positions; compare unqualified names structurally.
        return $type;
    }
}

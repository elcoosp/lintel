<?php

declare(strict_types=1);

namespace Lintel\PhpStan;

/** A bridge setup failure: a missing source, an undiscoverable binary. */
final class BridgeException extends \RuntimeException {}

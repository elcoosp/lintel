<?php

declare(strict_types=1);

namespace Lintel\Twig;

/** Thrown when a lintel_render() call names a component no source dir holds. */
final class BridgeException extends \RuntimeException {}

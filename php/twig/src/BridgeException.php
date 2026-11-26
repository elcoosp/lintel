<?php

declare(strict_types=1);

namespace Lintl\Twig;

/** Thrown when a lintl_render() call names a component no source dir holds. */
final class BridgeException extends \RuntimeException {}

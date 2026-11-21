<?php

declare(strict_types=1);

namespace Markup;

use InvalidArgumentException;

/** Thrown for rendering-contract violations (bad tag/attr names, hostile
 * values the contract rejects, unknown child types). */
final class Exception extends InvalidArgumentException
{
}

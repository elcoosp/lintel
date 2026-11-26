<?php

namespace Lintl\Fixtures;

const MODE = 'AT&T';

function Counts(int $count, $label)
{
    return <p>{$count} {$label}</p>;
}

function Price(float $price, $unit)
{
    return <span>{$price} {$unit}</span>;
}

/**
 * The consumer documents the proven shape (`numeric-string` — the type the
 * analyze pass trusts; PHPStan's level-max CI flags any non-numeric string
 * flowing into the parameter) and the body keeps the runtime guard: input
 * that is not numeric takes the rejected branch, so an elided child can
 * never see attacker markup (the threat case in `AnalyzeElisionTest`).
 *
 * @param numeric-string $code
 */
function Numeric($code)
{
    if (! is_numeric($code)) {
        return <p>bad</p>;
    }

    return <td>{$code}</td>;
}

function Literal()
{
    return <b>{MODE}</b>;
}

function Flags(bool $on)
{
    return <i>{$on}</i>;
}

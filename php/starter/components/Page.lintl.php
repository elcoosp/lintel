<?php

declare(strict_types=1);

namespace App;

use Lintl\Html;

/**
 * The Page component — the starter's second component. Demonstrates:
 *  - composition (a Card rendered as a child)
 *  - a fragment `<>...</>` (the lowering emits a concatenation)
 *  - a list of children (the array maps through the escaper)
 *  - the escape-by-default invariant on every dynamic value
 *
 * @param list<array{title: string, url: string}> $cards
 */
function Page(string $heading, array $cards): Html
{
    return <html lang="en">
        <body>
            <h1>{$heading}</h1>
            <main>
                {array_map(static fn (array $c) => <Card title={$c['title']} url={$c['url']} />, $cards)}
            </main>
        </body>
    </html>;
}

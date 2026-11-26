<?php

// The §11 Twig-bridge fixture: a function component the extension loads
// through the dev loader (compile-if-stale) and renders through c().
function Badge(string $name = '') {
    return <b class="badge">Hello, {$name}</b>;
}

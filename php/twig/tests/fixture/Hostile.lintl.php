<?php

// A component whose DYNAMIC payload is hostile — the runtime escapers own
// the contract, so the bridge's Twig output prints it escaped.
function Hostile() {
    return <em>{'<script>pwned</script>'}</em>;
}

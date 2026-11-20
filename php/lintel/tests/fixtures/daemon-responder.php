<?php

declare(strict_types=1);

// The T-12 daemon stand-in for the loader's socket lane: answers one
// request per connection from `argv[1]` (the socket path) in the mode
// named by `argv[2]` — `ok` (a compiled artifact) or `err` (rendered
// diagnostics). Exits after 5 idle seconds; the suite terminates it.

$socket = $argv[1] ?? '';
$mode = $argv[2] ?? 'ok';
$server = stream_socket_server('unix://' . $socket, $errno, $errstr);
if ($server === false) {
    fwrite(STDERR, (string) $errstr);
    exit(1);
}
while (($conn = @stream_socket_accept($server, 5)) !== false) {
    $line = fgets($conn);
    if ($line === false) {
        fclose($conn);
        continue;
    }
    if ($mode === 'ok') {
        $body = "<?php\n\nreturn 'from-daemon';\n";
        fwrite($conn, 'OK ' . strlen($body) . "\n" . $body);
    } elseif ($mode === 'err') {
        $body = 'LNT0101: rendered daemon diagnostics';
        fwrite($conn, 'ERR ' . strlen($body) . "\n" . $body);
    }
    fclose($conn);
}

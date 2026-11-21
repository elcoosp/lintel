<?php

declare(strict_types=1);

// The T-12 daemon stand-in for the loader's socket lane (`docs/
// daemon-protocol.md`): answers one framed request per connection from
// `argv[1]` (the socket path) in the mode named by `argv[2]` — `ok` (a
// compiled artifact) or `err` (rendered diagnostics). Framing is the
// daemon protocol's: ten ASCII decimal digits + `\n` + that many bytes of
// UTF-8 JSON; the request is `{"path":…}` (or `{"quit":true}`), the
// answer `{"ok":true,"php":…}` / `{"ok":false,"diag":[…]}`. Exits after 5
// idle seconds; the suite terminates it.

$socket = $argv[1] ?? '';
$mode = $argv[2] ?? 'ok';
$server = stream_socket_server('unix://' . $socket, $errno, $errstr);
if ($server === false) {
    fwrite(STDERR, (string) $errstr);
    exit(1);
}
while (($conn = @stream_socket_accept($server, 5)) !== false) {
    // The frame prefix: ten digits, then the payload.
    $prefix = '';
    while (strlen($prefix) < 10) {
        $chunk = fread($conn, 10 - strlen($prefix));
        if ($chunk === false || $chunk === '') {
            break;
        }
        $prefix .= $chunk;
    }
    if (strlen($prefix) === 10 && ctype_digit($prefix)) {
        // The separator newline between the prefix and the payload.
        $separator = @fread($conn, 1);
        if ($separator !== "\n") {
            fclose($conn);
            continue;
        }
        $length = (int) $prefix;
        $payload = '';
        while (strlen($payload) < $length) {
            $chunk = fread($conn, max(1, $length - strlen($payload)));
            if ($chunk === false || $chunk === '') {
                break;
            }
            $payload .= $chunk;
        }
        if (strlen($payload) === $length) {
            $request = json_decode($payload, true);
            if (is_array($request) && ($request['quit'] ?? false) === true) {
                $answer = ['ok' => true, 'quit' => true];
            } else {
                $answer = match ($mode) {
                    'ok' => ['ok' => true, 'php' => "<?php\n\nreturn 'from-daemon';\n"],
                    default => ['ok' => false, 'diag' => ['LNT0101: rendered daemon diagnostics']],
                };
            }
            $body = json_encode($answer, JSON_UNESCAPED_SLASHES);
            if ($body === false) {
                fclose($conn);
                continue;
            }
            fwrite($conn, sprintf('%010d', strlen($body)) . "\n" . $body);
        }
    }
    fclose($conn);
}

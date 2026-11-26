# The T-19 launch GIF (architecture doc §16): edit → refresh → compiled
# view, in one deterministic terminal session. Rendered by the CI
# `launch-assets` job (charmbracelet/vhs-action, which ships ttyd +
# ffmpeg) — locally you need `vhs` on PATH plus the lintl binary.
#
#   vhs docs/demo.tape
#
# Everything the tape types is a real shipped command: `lintl init`,
# `lintl build` — no staged fakes (§16 credibility rule).

Output docs/demo.gif

Set FontSize 21
Set Width 1180
Set Height 640
Set Shell bash
Set Theme { "name": "lintl", "background": "#181a20", "foreground": "#f0f0f5", "cursor": "#7c5cff", "black": "#3a3c46", "green": "#7ec699", "yellow": "#e5c07b", "blue": "#70b0ff", "magenta": "#c47cff", "cyan": "#6fc7c7", "white": "#dcdfe6" }

Type "cd $(mktemp -d)" Enter
Sleep 400ms

Type "lintl init" Enter
Sleep 900ms

Type "cat hello.lintl.php" Enter
Sleep 1s

Type "cat hello.php" Enter
Sleep 1.1s

Type "sed -i 's/Hello/Hello, Lintl!' hello.lintl.php" Enter
Sleep 300ms

Type "lintl build hello.lintl.php" Enter
Sleep 900ms

Type "cat hello.php" Enter
Sleep 1.6s

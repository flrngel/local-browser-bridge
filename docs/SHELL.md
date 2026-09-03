# Shell

Native shell execution on the server host. This grants a connected agent
**full current-user command authority** — read, launch, modify, or delete
anything your signed-in account can touch. It is not a sandbox, not scoped
to a browser tab or app window, and not part of the computer helper.

**This is on by default.** Any local program holding your bridge token (or
Agent Fetch URL) can already run commands as you, right after install, with
no separate setup step. If that is not what you want, turn it off before you
connect anything you do not fully trust.

## Turn it off (or back on)

The normal way, no restart needed: open the dashboard page or the
tray/menu-bar icon and use the **Shell access** switch. This writes
`settings.json` immediately — see
[Configuration](CONFIGURATION.md#settings-file).

Other ways to control it, all requiring a restart to take effect:

| Method | How |
|---|---|
| CLI flag | `--no-shell` (force off) or `--enable-shell` (force on), on the console server |
| Environment variable | `LBB_ENABLE_SHELL=1` (force on; accepts `1/true/yes/on`) |
| macOS installer | `install-macos.sh -- --enable-shell` bakes it into the LaunchAgent |
| Windows installer | `install-windows.ps1 -EnableShell` bakes it into the Startup shortcut |

A CLI flag or environment variable always wins for that run, even over a
`settings.json` that says otherwise; it does not edit the file. See
[Configuration](CONFIGURATION.md) for exact flag/variable names across all
three executables.

## Methods

`shell.status` always works and reports whether shell authority is enabled,
without needing it. `shell.run` executes a command and needs shell authority;
without it, every call returns 403 `SHELL_DISABLED` (verified):

```json
{"callId":"...","error":{"code":"SHELL_DISABLED","message":"Local shell access is disabled; restart the server with --enable-shell to grant it"},"ok":false,"taxonomy":{"code":"blocked_by_policy","prose":"Bridge policy forbids this request; do not retry the same action.","recoveryHint":"none","retriable":false}}
```

Full method signatures: [API reference](API_REFERENCE.md#shell-methods-shell_methods-srcshellrs).

## Request

```bash
curl -s -X POST http://127.0.0.1:17373/api/v1/command \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"method":"shell.run","callId":"shell-1","params":{"command":"pwd","timeoutMs":5000}}'
```

`shell` selects the interpreter: `default` (platform default), `zsh` or `sh`
on macOS, `powershell` or `cmd` on Windows — the other platform's names fail
`SHELL_UNSUPPORTED` (verified: `"powershell and cmd are not supported by the
macOS package"`). `cwd` is optional.

## Limits

| Limit | Value |
|---|---|
| Command size | 16 KiB |
| Output retained per stream (stdout/stderr) | 1 MiB, excess drained and marked `*Truncated: true` |
| Default timeout | 30 seconds |
| Maximum timeout (`timeoutMs`) | 120 seconds |
| stdin | always null (non-interactive) |

A timeout terminates the whole process tree and returns `timedOut: true`
(verified: a 5-second `sleep` with `timeoutMs: 500` returns
`"timedOut":true,"exitCode":null` at ~502ms). A nonzero exit code is a
**completed** command result, not a transport error — check `exitCode`, not
just `ok`.

## What is logged

The activity log records only that a shell command ran and its completion
status — never the command text, working directory, stdout, or stderr.

## Idempotency

Like every other method, `shell.run` participates in `callId` replay: the same
`callId` with the same parameters returns the cached result instead of
running the command again; the same `callId` with different parameters fails
`CALL_ID_REUSED` (verified). This does not make a command idempotent by
itself — `rm -rf` replayed with a *new* `callId` still runs again. See
[Agent integration](AGENT_INTEGRATION.md#callid-idempotency-replay-and-cancellation).

## Why this exists, and why it is separate

Some agent tasks (running the project's own test suite, checking a build)
need real command execution, not simulated browser or window control. Keeping
it a separate, independently switchable capability — rather than folding it
into the computer helper's exact-window model — means turning desktop
control off never silently revokes shell authority, and vice versa; each has
its own switch.

There is no per-command prompt, filesystem allowlist, container, or privilege
reduction. See [Security](../SECURITY.md) for the full trust model.

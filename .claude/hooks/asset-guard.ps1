# asset-guard.ps1 - PreToolUse hook (Bash / PowerShell tools)
# Asks the owner before any access to the video asset tree (under iCloudDrive).
#
# Why: 2026-09-21 the assistant rebuilt a chapter from a stale copy in that tree
# and overwrote a good deliverable, reverting a fix approved the day before.
# Owner instruction: "ask me for permission when you touch the assets".
#
# The first version denied outright (exit 2) instead of asking, so granting
# permission had no effect and the owner-designated work folder stayed
# unreachable. That was an implementation error, corrected here.
#
# Match is ASCII-only on purpose. Two reasons:
#   - powershell.exe 5.1 misreads BOM-less UTF-8 source, so Japanese literals
#     corrupt the script (git-guard.ps1 says the same at its head).
#   - stdin is decoded with the console code page, so Japanese inside the
#     command string arrives mangled and can never match a Japanese pattern.
# The asset tree is the only thing under iCloudDrive here, so that name is a
# safe ASCII discriminator; the baseline lives under Documents\xoksa.
#
# Exit 0 with permissionDecision=ask -> the owner is prompted for each access.
# Exit 0 with no output = allow (normal permission flow still applies).
$ErrorActionPreference = 'Continue'

$raw = [Console]::In.ReadToEnd()
try { $j = $raw | ConvertFrom-Json } catch { exit 0 }
if ($j.tool_name -ne 'Bash' -and $j.tool_name -ne 'PowerShell') { exit 0 }
$cmd = [string]$j.tool_input.command
if ([string]::IsNullOrWhiteSpace($cmd)) { exit 0 }

if ($cmd -match '(?i)iclouddrive') {
    $out = [ordered]@{
        hookSpecificOutput = [ordered]@{
            hookEventName            = 'PreToolUse'
            permissionDecision       = 'ask'
            permissionDecisionReason = 'asset-guard: this command touches the video asset tree. Owner approval required.'
        }
    }
    Write-Output ($out | ConvertTo-Json -Depth 5 -Compress)
    exit 0
}
exit 0

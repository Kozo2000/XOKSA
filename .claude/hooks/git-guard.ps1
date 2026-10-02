# git-guard.ps1 - PreToolUse hook (Bash / PowerShell tools)
# Enforces docs/dev-oper/github-operations-policy.md mechanically:
#   - blocks force push (incl. --force-with-lease, -f, +refspec, --mirror)
#   - blocks any push targeting main
#   - blocks rebase / reset --hard / branch -D / tag -d / remote ref deletion / gh pr merge
#   - new branches must start at the tip of origin/main (feature branch = branch from main)
# Exit 2 + stderr = deny. Exit 0 = allow (normal permission flow still applies).
# Messages are ASCII-only: powershell.exe 5.1 misreads BOM-less UTF-8 sources.
#
# The checks read the COMMAND, not the data the command writes. A here-document
# body (cat > file << 'EOF' ... EOF) is data, so it is removed before matching;
# otherwise writing a document, a commit message or a PR body that MENTIONS a
# blocked operation is denied, which is a false positive. The body is KEPT when
# the command consuming it is a shell or an interpreter, because there the body
# really is executed. No check below is loosened by this.
$ErrorActionPreference = 'Continue'

$raw = [Console]::In.ReadToEnd()
try { $j = $raw | ConvertFrom-Json } catch { exit 0 }
if ($j.tool_name -ne 'Bash' -and $j.tool_name -ne 'PowerShell') { exit 0 }
$cmd = [string]$j.tool_input.command
if ([string]::IsNullOrWhiteSpace($cmd)) { exit 0 }

function Deny([string]$msg) {
    [Console]::Error.WriteLine("git-guard BLOCK: $msg")
    exit 2
}

# Commands whose here-document body is executed rather than stored. For these the
# body stays in the scanned text.
$InterpreterRe = '(?:^|[\n;&|(`]|&&|\|\|)\s*(?:sudo\s+)?(?:env\s+\S+=\S+\s+)*(?:bash|sh|zsh|ksh|dash|python[0-9.]*|py|powershell(?:\.exe)?|pwsh|node|perl|ruby|php|Rscript|osascript)\b'

function Remove-HeredocBodies([string]$text) {
    if ($text -notmatch '<<') { return $text }
    $lines = $text -split "`n", 0
    $out = New-Object System.Collections.Generic.List[string]
    $delim = $null
    $strip = $false
    foreach ($line in $lines) {
        if ($null -ne $delim) {
            if ($line.Trim() -eq $delim) { $delim = $null; $strip = $false; continue }
            if (-not $strip) { $out.Add($line) }
            continue
        }
        $out.Add($line)
        $m = [regex]::Match($line, '<<-?\s*(?:''([^'']+)''|"([^"]+)"|([A-Za-z_][A-Za-z0-9_]*))')
        if ($m.Success) {
            $delim = if ($m.Groups[1].Success) { $m.Groups[1].Value }
                     elseif ($m.Groups[2].Success) { $m.Groups[2].Value }
                     else { $m.Groups[3].Value }
            # Body is data unless an interpreter consumes it.
            $strip = -not ($line -match $InterpreterRe)
        }
    }
    return ($out -join "`n")
}

$cmd = Remove-HeredocBodies $cmd

# --- force push in any spelling ---
if ($cmd -match 'git\s+push\b[^|;&]*(\s-f\b|\s--force|\s--mirror)') {
    Deny 'force push is prohibited by the canon (github-operations-policy.md NG list).'
}
if ($cmd -match 'git\s+push\b[^|;&]*\s\+\S') {
    Deny 'push with +refspec is a force push; prohibited by the canon.'
}

# --- push targeting main ---
if ($cmd -match 'git\s+push\b[^|;&]*\bmain\b') {
    Deny 'pushing to main is prohibited; main changes go through a PR only.'
}

# --- history rewrite / destructive ops ---
if ($cmd -match 'git\s+rebase\b')          { Deny 'rebase (history rewrite) is blocked in this environment.' }
if ($cmd -match 'git\s+reset\s+--hard')    { Deny 'git reset --hard is blocked.' }
# -cmatch, not -match: PowerShell matches case-insensitively by default, so the
# -D rule also caught -d. Only -D forces; -d refuses to delete an unmerged branch
# and is the canon's own cleanup step ("feature branches are short-lived").
if ($cmd -cmatch 'git\s+branch\s+-D\b')    { Deny 'force branch deletion is blocked. Use -d (refuses unmerged branches).' }
if ($cmd -match 'git\s+tag\s+(-d|--delete)\b') { Deny 'tag deletion is blocked.' }
if ($cmd -match 'git\s+push\b[^|;&]*--delete') { Deny 'remote ref deletion is blocked.' }
if ($cmd -match 'gh\s+pr\s+merge\b')       { Deny 'merging is the maintainer''s operation on GitHub (canon).' }

# --- new branch must start at the tip of origin/main ---
$newBranch = ($cmd -match 'git\s+checkout\s+(-b|-B)\b') -or
             ($cmd -match 'git\s+switch\s+(-c|-C|--create)\b')
# Naming origin/main as the start point satisfies the canon directly, so it does
# not matter where HEAD stands:  git switch -c <new> origin/main
# Without this, the only way through was to stand on main first, which pushed the
# operator onto main for no reason.
$explicitMain = $cmd -match 'git\s+(checkout\s+(-b|-B)|switch\s+(-c|-C|--create))\s+\S+\s+origin/main\b'
if ($newBranch -and -not $explicitMain) {

#if ($newBranch) {
    $cwd = if ($j.cwd) { [string]$j.cwd } else { (Get-Location).Path }
    $head    = (& git -C $cwd rev-parse HEAD 2>$null | Select-Object -First 1)
    $mainTip = (& git -C $cwd rev-parse origin/main 2>$null | Select-Object -First 1)
    if (-not $head -or -not $mainTip) {
        Deny 'cannot resolve HEAD / origin/main. Run git fetch origin first, then retry.'
    }
    if ($head -ne $mainTip) {
        Deny ("new branch must start at the tip of origin/main (canon: feature branch = branch from main). HEAD=$head origin/main=$mainTip. Run: git fetch origin, stand on the main tip, then create the branch.")
    }
}

exit 0

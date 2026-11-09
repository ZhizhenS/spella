# Compile/verify the reviewed embedded bigram asset before any Cargo build.
[CmdletBinding()]
param(
    [string]$PythonPath = $env:ENGLISH_ASSIST_PYTHON,
    [switch]$VerifyOnly,
    [switch]$Test
)
$ErrorActionPreference = 'Stop'
if (-not $PythonPath) {
    $taskCandidates = @('C:\Python313\python.exe')
    foreach ($taskCommand in @('python.exe', 'python3.exe')) {
        $taskFound = Get-Command $taskCommand -ErrorAction SilentlyContinue
        if ($taskFound -and $taskFound.Source -notlike '*\WindowsApps\*') { $taskCandidates += $taskFound.Source }
    }
    $PythonPath = $taskCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
}
if (-not $PythonPath -or -not (Test-Path -LiteralPath $PythonPath -PathType Leaf)) {
    throw 'Python 3.11+ is required for the embedded bigram asset. Set ENGLISH_ASSIST_PYTHON to its executable.'
}
& $PythonPath -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)'
if ($LASTEXITCODE -ne 0) { throw 'Python 3.11+ is required for the embedded bigram asset.' }
if ($Test) {
    & $PythonPath (Join-Path $PSScriptRoot 'test-build-bigrams.py')
    if ($LASTEXITCODE -ne 0) { throw 'Embedded bigram compiler/integrity tests failed.' }
}
$taskArguments = @((Join-Path $PSScriptRoot 'build-bigrams.py'))
if ($VerifyOnly) { $taskArguments += '--verify-only' }
& $PythonPath @taskArguments
if ($LASTEXITCODE -ne 0) { throw 'Frozen embedded bigram build/verification failed.' }

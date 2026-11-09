# Offline build: all source data and licenses must already be present in this checkout.
[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [string]$PythonPath = $env:ENGLISH_ASSIST_PYTHON,
    [switch]$VerifyOnly,
    [switch]$Test
)
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $taskRepo 'dist\english-dictionaries' }
if (-not $PythonPath) {
    $taskCandidates = @('C:\Python313\python.exe')
    foreach ($taskCommand in @('python.exe', 'python3.exe')) {
        $taskFound = Get-Command $taskCommand -ErrorAction SilentlyContinue
        if ($taskFound -and $taskFound.Source -notlike '*\WindowsApps\*') { $taskCandidates += $taskFound.Source }
    }
    $PythonPath = $taskCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
}
if (-not $PythonPath -or -not (Test-Path -LiteralPath $PythonPath -PathType Leaf)) {
    throw 'Python 3.11+ is required to build the frozen offline lexicon. Set ENGLISH_ASSIST_PYTHON to its executable.'
}
& $PythonPath -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)'
if ($LASTEXITCODE -ne 0) { throw 'Python 3.11+ is required to build the frozen offline lexicon.' }
if ($Test) {
    & $PythonPath (Join-Path $PSScriptRoot 'test-prepare-lexicon.py')
    if ($LASTEXITCODE -ne 0) { throw 'Offline lexicon packaging integrity tests failed.' }
    & $PythonPath (Join-Path $PSScriptRoot 'test-build-lexicon.py')
    if ($LASTEXITCODE -ne 0) { throw 'Offline lexicon compiler tests failed.' }
}
$taskArguments = @((Join-Path $PSScriptRoot 'prepare-lexicon.py'), '--output', $OutputDirectory)
if ($VerifyOnly) { $taskArguments += '--verify-only' }
& $PythonPath @taskArguments
if ($LASTEXITCODE -ne 0) { throw 'Frozen offline lexicon build/verification failed.' }

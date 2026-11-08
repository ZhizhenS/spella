# Prepare the frozen upstream English data without installing or executing any software.
[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $taskRepo 'dist\english-dictionaries' }
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
$taskCatalog = Get-Content -LiteralPath (Join-Path $taskRepo 'data\dict\resources.json') -Raw | ConvertFrom-Json
$taskEnglish = @($taskCatalog.components | Where-Object id -eq 'english')
if ($taskEnglish.Count -ne 1) { throw 'Expected exactly one English vocabulary component.' }
$taskEnglish = $taskEnglish[0]
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
$taskName = 'retype-english-' + $taskEnglish.sha256 + '.bin.gz'
$taskArchive = Join-Path $taskOutput $taskName
$taskImage = Join-Path $taskOutput ('english-' + $taskEnglish.sha256 + '.bin')
if (-not (Test-Path -LiteralPath $taskArchive)) {
    Invoke-WebRequest -Uri ('https://github.com/L-Chris/retype/releases/download/v0.8.13/' + $taskName) -OutFile $taskArchive
}
if ((Get-Item -LiteralPath $taskArchive).Length -ne $taskEnglish.compressedBytes -or
    (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash -ne $taskEnglish.compressedSha256) {
    throw 'Frozen compressed English vocabulary failed its catalog size/hash check.'
}
$taskCompressed = [IO.File]::OpenRead($taskArchive)
try {
    $taskGzip = [IO.Compression.GZipStream]::new($taskCompressed, [IO.Compression.CompressionMode]::Decompress)
    try {
        $taskTarget = [IO.File]::Create($taskImage)
        try { $taskGzip.CopyTo($taskTarget) } finally { $taskTarget.Dispose() }
    } finally { $taskGzip.Dispose() }
} finally { $taskCompressed.Dispose() }
if ((Get-Item -LiteralPath $taskImage).Length -ne $taskEnglish.bytes -or
    (Get-FileHash -LiteralPath $taskImage -Algorithm SHA256).Hash -ne $taskEnglish.sha256) {
    throw 'Frozen English vocabulary failed its uncompressed size/hash check.'
}
$taskBytes = [IO.File]::ReadAllBytes($taskImage)
if ([Text.Encoding]::ASCII.GetString($taskBytes, 0, 8) -ne 'RTENGL01') { throw 'Unexpected English vocabulary format.' }
[pscustomobject]@{ Path=$taskImage; Bytes=$taskBytes.Length; SHA256=$taskEnglish.sha256; Format='RTENGL01'; Verified=$true } | ConvertTo-Json

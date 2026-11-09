# File-integrity policy tests in one temporary repository. No Cargo, ISCC or install.
# Native-named files below are explicitly non-PE unit fixtures, never an installer payload.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\frozen-payload.ps1')
$script:Passed = 0
function Assert-True([bool]$Value, [string]$Name) {
  if (-not $Value) { throw $Name }; $script:Passed++
}
function Assert-Denied([scriptblock]$Action, [string]$Name) {
  $denied = $false
  try { $null = & $Action } catch { $denied = $true }
  Assert-True $denied $Name
}
function Write-Fixture([string]$Path, [string]$Content) {
  [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path)) | Out-Null
  [IO.File]::WriteAllText($Path, $Content, [Text.UTF8Encoding]::new($false))
}
$taskTempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$taskRoot = Join-Path $taskTempBase ('EnglishAssist-frozen-test-' + [Guid]::NewGuid().ToString('N'))
$taskRepo = Join-Path $taskRoot 'repo'
$taskStage = Join-Path $taskRepo ('dist\.english-assist-freeze-' + [Guid]::NewGuid().ToString('N'))
try {
  Write-Fixture (Join-Path $taskRepo 'Cargo.toml') "[workspace.package]`nversion = `"0.2.0`"`n[other]`nversion = `"9.0.0`"`n"
  Write-Fixture (Join-Path $taskRepo 'tools/english-assist/build-lexicon.py') '# compiler unit fixture'
  Assert-True ((Get-EnglishAssistSourceVersion $taskRepo) -eq '0.2.0') 'Workspace source version must be authoritative'
  $spec = @(Get-EnglishAssistPayloadSpec)
  Assert-True ($spec.Count -eq 24) 'Full explicit payload file list'
  foreach ($entry in $spec) {
    Write-Fixture (Join-Path $taskRepo $entry.source) ('non-executable unit fixture: ' + $entry.path)
  }
  $lexiconPath = Join-Path $taskRepo 'dist/english-dictionaries/english-lexicon-v1.bin'
  $lexicon = [ordered]@{schema=1;format='RTENLX01';file='english-lexicon-v1.bin';bytes=([IO.FileInfo]$lexiconPath).Length;sha256=(Get-EnglishAssistPayloadHash $lexiconPath)}
  Write-Fixture (Join-Path $taskRepo 'dist/english-dictionaries/english-lexicon-v1.json') ($lexicon | ConvertTo-Json)
  foreach ($group in @('x64','learning','ai','x86','support')) { Copy-EnglishAssistProductionGroup $taskRepo $taskStage $group }
  Complete-EnglishAssistPayload $taskRepo $taskStage '0.2.0'
  $originalText = [IO.File]::ReadAllText((Join-Path $taskStage 'build.json'))
  $metadata = Assert-EnglishAssistFrozenPayload $taskStage '0.2.0'
  Assert-True ($metadata.artifacts.Count -eq 24) 'Every packaged file has bytes/hash'
  Assert-True (@($metadata.artifacts | Where-Object { $_.kind -in @('tip-x64','tip-x86','settings','helper') }).Count -eq 6) 'All six production DLL/EXE files recorded'
  Assert-True ($metadata.metadataCompilerSha256.Length -eq 64) 'Python compiler digest recorded'
  Assert-True ($metadata.lexicon.sha256 -eq (Get-EnglishAssistPayloadHash (Join-Path $taskStage 'dictionaries/english-lexicon-v1.bin'))) 'Python metadata binary digest recorded'
  Assert-Denied { Assert-EnglishAssistFrozenPayload $taskStage '0.3.0' } 'Repack cannot relabel source version'
  Assert-Denied { Complete-EnglishAssistPayload $taskRepo $taskStage '0.3.0' } 'Freeze version must match source'

  $frozenNative = Join-Path $taskStage 'retype_ime.dll'
  $nativeHash = Get-EnglishAssistPayloadHash $frozenNative
  Write-Fixture (Join-Path $taskRepo 'target/release/retype_ime.dll') 'different later tool-build fixture'
  $null = Assert-EnglishAssistFrozenPayload $taskStage '0.2.0'
  Assert-True ((Get-EnglishAssistPayloadHash $frozenNative) -eq $nativeHash) 'Later target overwrite cannot affect frozen production native'
  Write-Fixture (Join-Path $taskRepo 'platforms/windows/installer/user-profile.ps1') '# explicitly reviewed script-only revision'
  Update-EnglishAssistFrozenScripts $taskRepo $taskStage '0.2.0'
  $updated = Assert-EnglishAssistFrozenPayload $taskStage '0.2.0'
  Assert-True ((Get-EnglishAssistPayloadHash $frozenNative) -eq $nativeHash) 'Script repack never refreshes native from target'
  Assert-True ([IO.File]::ReadAllText((Join-Path $taskStage 'user-profile.ps1')) -eq '# explicitly reviewed script-only revision') 'Script refresh is explicit and updates its hash'
  $originalText = [IO.File]::ReadAllText((Join-Path $taskStage 'build.json'))
  $nativeBytes = [IO.File]::ReadAllBytes($frozenNative)
  Write-Fixture $frozenNative 'corrupted native fixture'
  Assert-Denied { Assert-EnglishAssistFrozenPayload $taskStage '0.2.0' } 'Changed native size/hash rejected'
  Assert-Denied { Update-EnglishAssistFrozenScripts $taskRepo $taskStage '0.2.0' } 'Script refresh cannot bless changed native'
  [IO.File]::WriteAllBytes($frozenNative, $nativeBytes)
  $file = Join-Path $taskStage 'x86/retype_ime.dll'
  $bytes = [IO.File]::ReadAllBytes($file); [IO.File]::Delete($file)
  Assert-Denied { Assert-EnglishAssistFrozenPayload $taskStage '0.2.0' } 'Missing x86 native rejected'
  [IO.File]::WriteAllBytes($file, $bytes)
  foreach ($mutation in @('duplicate','traversal','sourceVersion','lexicon')) {
    $bad = ConvertFrom-Json $originalText
    switch ($mutation) {
      'duplicate' { $bad.artifacts[1].path = $bad.artifacts[0].path }
      'traversal' { $bad.artifacts[0].path = '../outside.dll' }
      'sourceVersion' { $bad.sourceVersion = '0.1.0' }
      'lexicon' { $bad.lexicon.sha256 = '0' * 64 }
    }
    Write-EnglishAssistPayloadManifest $taskStage $bad
    Assert-Denied { Assert-EnglishAssistFrozenPayload $taskStage '0.2.0' } "Invalid manifest rejected: $mutation"
    Write-Fixture (Join-Path $taskStage 'build.json') $originalText
  }
  Assert-Denied { Publish-EnglishAssistFrozenPayload $taskRepo (Join-Path $taskRoot 'elsewhere') '0.2.0' } 'Publish requires the dedicated staging directory'
  $published = Publish-EnglishAssistFrozenPayload $taskRepo $taskStage '0.2.0'
  Assert-True ($published -eq (Join-Path $taskRepo 'dist\english-assist')) 'Publish dedicated frozen directory'
  $null = Assert-EnglishAssistFrozenPayload $published '0.2.0'
  Assert-True ((Get-EnglishAssistPayloadHash (Join-Path $published 'retype_ime.dll')) -eq $nativeHash) 'Published artifact remains the original production freeze'
  $secondStage = Join-Path $taskRepo ('dist\.english-assist-freeze-' + [Guid]::NewGuid().ToString('N'))
  foreach ($relative in @($spec.path) + @('build.json')) {
    $destination = Join-Path $secondStage $relative
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
    [IO.File]::Copy((Join-Path $published $relative), $destination)
  }
  $null = Publish-EnglishAssistFrozenPayload $taskRepo $secondStage '0.2.0'
  $previous = @([IO.Directory]::EnumerateDirectories((Join-Path $taskRepo 'dist'),'english-assist.previous-*'))
  Assert-True ($previous.Count -eq 1) 'A replacement freeze preserves the preceding complete directory'
  Assert-True ((Get-EnglishAssistPayloadHash (Join-Path $previous[0] 'retype_ime.dll')) -eq $nativeHash) 'Previous native artifact retained without deletion'

  # Verify the production entry ordering and the installer's actual source set.
  $build = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\build-english-assist.ps1'))
  Assert-True ($build.IndexOf("Copy-EnglishAssistProductionGroup `$taskRepo `$taskStage 'x86'") -lt $build.IndexOf('cargo run')) 'Native freeze occurs before any resource tool'
  Assert-True ($build.IndexOf('Publish-EnglishAssistFrozenPayload') -lt $build.IndexOf('cargo test')) 'Frozen payload published before Rust tests'
  $repack = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\repack-english-assist.ps1'))
  Assert-True ($repack -notmatch 'target[\\/]|cargo\s+(build|run|test)') 'Repack has no target/native build fallback'
  $iss = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\english-assist.iss'))
  Assert-True ($iss.Contains('SourceDir={#PayloadRoot}') -and $iss -notmatch 'BaseDir|X86Dir|target\\') 'ISCC requires frozen payload only'
  $sources = @([regex]::Matches($iss, '(?m)^Source: "([^"]+)"') | ForEach-Object { $_.Groups[1].Value.Replace('\','/') })
  Assert-True ($sources.Count -eq ($spec.Count + 1)) 'ISCC file count equals full frozen manifest plus build.json'
  foreach ($source in $sources) { Assert-True ($source -eq 'build.json' -or $source -cin $spec.path) "ISCC source is frozen: $source" }
  Assert-True ($iss.Contains('Flags: ignoreversion regserver 64bit uninsrestartdelete') -and $iss.Contains('Flags: ignoreversion regserver 32bit uninsrestartdelete')) 'Native registration flags retained'
  Write-Output "$script:Passed frozen payload/repack policy cases passed; temporary fixtures only, no build, ISCC or installation."
} finally {
  $resolved = [IO.Path]::GetFullPath($taskRoot).TrimEnd('\')
  if (-not $resolved.StartsWith($taskTempBase + '\', [StringComparison]::OrdinalIgnoreCase) -or
      [IO.Path]::GetFileName($resolved) -notmatch '^EnglishAssist-frozen-test-[a-f0-9]{32}$') { throw 'Unsafe test cleanup path.' }
  if ([IO.Directory]::Exists($resolved)) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}

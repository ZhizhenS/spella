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
  Write-Fixture (Join-Path $taskRepo 'tools/english-assist/modern-entities.py') '# entity compiler unit fixture'
  Write-Fixture (Join-Path $taskRepo 'data/dict/raw/modern-entities/SOURCES.json') '{"unitFixture":true}'
  $bigram = [ordered]@{schema=1;format='RTENBG01';file='bigrams.bin';bytes=128;sha256=('a'*64);compiler_sha256=('b'*64);source_manifest_sha256=('f'*64);counts=@{words=2;pairs=1};source=@{repository='https://github.com/wolfgarbe/SymSpell';commit=('c'*40);path='SymSpell/frequency_bigramdictionary_en_243_342.txt';sha256=('d'*64);uncompressedSha256=('e'*64)}}
  Write-Fixture (Join-Path $taskRepo 'core/english/assets/bigrams.json') ($bigram | ConvertTo-Json -Depth 8)
  $entities=[ordered]@{schema=2;runtime_format='EAMODEN2';data_license='CC0-1.0';snapshot=@{file='raw/modern-entities/wikidata-expanded-snapshot.json.gz';sha256=('c'*64);content_sha256=('d'*64)};reviewed_seed=@{file='raw/modern-entities/reviewed-seed-v1.json';sha256=('a'*64);count=1};discovered_count=32;rejected=@{no_chinese=2};entries=@(1..30 | ForEach-Object {@{text=('Fixture'+$_)}})}
  Write-Fixture (Join-Path $taskRepo 'data/dict/modern-entities.json') ($entities | ConvertTo-Json -Depth 8)
  $runtime="# EAMODEN2`ttext`taliases`tkind`tdefinition_zh`twikidata_id`trevision`tofficial_url`tpriority`n"
  foreach ($index in 1..30) { $runtime+="Fixture$index`tfixture$index`tentity`tfixture`tQ$index`t1`t`t100`n" }
  Write-Fixture (Join-Path $taskRepo 'data/dict/modern-entities.tsv') $runtime
  $modernRuntime="# EAMODLX1`tkey`ttext`tpos`tdefinition_zh`tusage`tfull_form`tsource_url`tsense_id`tpriority`n"
  foreach ($index in 1..5) { $modernRuntime+="fixture`tFixture`tn`tdefinition`tusage`tfull`thttps://example.invalid/`tsense$index`t100`n" }
  $modernPath=Join-Path $taskRepo 'data/dict/modern-lexicon.tsv'
  Write-Fixture $modernPath $modernRuntime
  $modern=[ordered]@{schema=1;format='EAMODLX1';runtime_format='EAMODLX1';file='modern-lexicon.tsv';data_license='CC-BY-SA-4.0';bytes=([IO.FileInfo]$modernPath).Length;sha256=(Get-EnglishAssistPayloadHash $modernPath);producerSha256=('1'*64);source_sha256=('2'*64);chinese_source_sha256=('3'*64);counts=@{entries=3;senses=5;distinct_keys=2}}
  Write-Fixture (Join-Path $taskRepo 'data/dict/modern-lexicon.json') ($modern | ConvertTo-Json -Depth 8)
  Assert-True ((Get-EnglishAssistSourceVersion $taskRepo) -eq '0.2.0') 'Workspace source version must be authoritative'
  $spec = @(Get-EnglishAssistPayloadSpec)
  Assert-True ($spec.Count -eq 29) 'Full explicit payload file list'
  Assert-True (@($spec | Where-Object { $_.path -ceq 'LICENSE-Wanxiang' -and $_.source -ceq 'data/dict/raw/wanxiang-base/LICENSE' -and $_.kind -eq 'license' }).Count -eq 1) 'Complete Chinese frequency source license is a frozen artifact'
  Assert-True (@($spec | Where-Object { $_.path -ceq 'LICENSE-Wikidata-CC0-1.0.txt' -and $_.kind -eq 'license' }).Count -eq 1) 'Full CC0 license is a frozen artifact'
  Assert-True (@($spec | Where-Object { $_.path -ceq 'LICENSE-Wiktionary-CC-BY-SA-4.0.html' -and $_.kind -eq 'license' }).Count -eq 1) 'Full Wiktionary data license is a frozen artifact'
  foreach ($entry in $spec) {
    Write-Fixture (Join-Path $taskRepo $entry.source) ('non-executable unit fixture: ' + $entry.path)
  }
  $lexiconPath = Join-Path $taskRepo 'dist/english-dictionaries/english-lexicon-v2.bin'
  $lexicon = [ordered]@{schema=2;format='RTENLX02';file='english-lexicon-v2.bin';bytes=([IO.FileInfo]$lexiconPath).Length;sha256=(Get-EnglishAssistPayloadHash $lexiconPath)}
  Write-Fixture (Join-Path $taskRepo 'dist/english-dictionaries/english-lexicon-v2.json') ($lexicon | ConvertTo-Json)
  foreach ($group in @('x64','learning','ai','x86','support')) { Copy-EnglishAssistProductionGroup $taskRepo $taskStage $group }
  Complete-EnglishAssistPayload $taskRepo $taskStage '0.2.0'
  $originalText = [IO.File]::ReadAllText((Join-Path $taskStage 'build.json'))
  $metadata = Assert-EnglishAssistFrozenPayload $taskStage '0.2.0'
  Assert-True ($metadata.artifacts.Count -eq 29) 'Every packaged file has bytes/hash'
  Assert-True (@($metadata.artifacts | Where-Object { $_.kind -in @('tip-x64','tip-x86','settings','helper') }).Count -eq 6) 'All six production DLL/EXE files recorded'
  Assert-True ($metadata.metadataCompilerSha256.Length -eq 64) 'Python compiler digest recorded'
  Assert-True ($metadata.EnglishContinuous.sha256 -eq ('a'*64)) 'Embedded bigram image digest retained without another runtime artifact'
  Assert-True ($metadata.ModernEntities.count -eq 30 -and $metadata.ModernEntities.data_license -ceq 'CC0-1.0') 'Embedded entity count/license retained without another runtime artifact'
  Assert-True ($metadata.ModernEntities.approved.sha256 -eq (Get-EnglishAssistPayloadHash (Join-Path $taskRepo 'data/dict/modern-entities.json'))) 'Reviewed entity JSON digest recorded'
  Assert-True ($metadata.ModernEntities.runtime.sha256 -eq (Get-EnglishAssistPayloadHash (Join-Path $taskRepo 'data/dict/modern-entities.tsv'))) 'Embedded entity TSV digest recorded'
  Assert-True ($metadata.ModernLexicon.counts.senses -eq 5 -and $metadata.ModernLexicon.data_license -ceq 'CC-BY-SA-4.0') 'Admitted modern senses and selected data license recorded'
  Assert-True ($metadata.ModernLexicon.runtime.sha256 -eq (Get-EnglishAssistPayloadHash $modernPath)) 'Embedded modern lexicon digest recorded'
  Assert-True ($metadata.lexicon.sha256 -eq (Get-EnglishAssistPayloadHash (Join-Path $taskStage 'dictionaries/english-lexicon-v2.bin'))) 'Python metadata binary digest recorded'
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
  foreach ($mutation in @('duplicate','traversal','sourceVersion','lexicon','lexiconSchema','lexiconFile','lexiconFormat','bigram','bigramSource','entityCount','entityDigest','entityLicense','entitySource','entitySeed','modernCount','modernDigest','modernLicense','modernSource')) {
    $bad = ConvertFrom-Json $originalText
    switch ($mutation) {
      'duplicate' { $bad.artifacts[1].path = $bad.artifacts[0].path }
      'traversal' { $bad.artifacts[0].path = '../outside.dll' }
      'sourceVersion' { $bad.sourceVersion = '0.1.0' }
      'lexicon' { $bad.lexicon.sha256 = '0' * 64 }
      'lexiconSchema' { $bad.lexicon.schema = 1 }
      'lexiconFile' { $bad.lexicon.file = 'english-lexicon-v1.bin' }
      'lexiconFormat' { $bad.lexicon.format = 'RTENLX01' }
      'bigram' { $bad.EnglishContinuous.format = 'unknown' }
      'bigramSource' { $bad.EnglishContinuous.source.commit = 'unreviewed' }
      'entityCount' { $bad.ModernEntities.count = 0 }
      'entityDigest' { $bad.ModernEntities.runtime.sha256 = 'unreviewed' }
      'entityLicense' { $bad.ModernEntities.data_license = 'unknown' }
      'entitySource' { $bad.ModernEntities.snapshot.file = '../outside.json.gz' }
      'entitySeed' { $bad.ModernEntities.reviewed_seed.sha256 = 'unreviewed' }
      'modernCount' { $bad.ModernLexicon.counts.senses = 0 }
      'modernDigest' { $bad.ModernLexicon.runtime.sha256 = 'unreviewed' }
      'modernLicense' { $bad.ModernLexicon.data_license = 'unknown' }
      'modernSource' { $bad.ModernLexicon.source_sha256 = 'unreviewed' }
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
  Assert-True ($build.IndexOf("'tools\english-assist\modern-entities.py'") -ge 0 -and $build.IndexOf("'tools\english-assist\modern-entities.py'") -lt $build.IndexOf('cargo build') -and $build.Contains('--check')) 'Reviewed entity check precedes production compilation'
  Assert-True ($build.IndexOf("'tools\english-assist\build-modern-lexicon.py'") -ge 0 -and $build.IndexOf("'tools\english-assist\build-modern-lexicon.py'") -lt $build.IndexOf('cargo build')) 'Strict modern lexicon check precedes production compilation'
  Assert-True ($build.Contains('retype-engine --test modern -- --ignored')) 'Prepared modern commit integration is an explicit formal gate'
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

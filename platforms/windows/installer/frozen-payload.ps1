# Build/repack helpers only. Importing does not copy files or run any tools.
function Get-EnglishAssistSourceVersion([string]$RepoRoot) {
  $toml = [IO.File]::ReadAllText((Join-Path $RepoRoot 'Cargo.toml'))
  $section = [regex]::Match($toml, '(?ms)^\[workspace\.package\]\s*\r?\n(.*?)(?=^\[|\z)').Groups[1].Value
  $version = [regex]::Match($section, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+(?:\.[0-9]+)?)"').Groups[1].Value
  if (-not $version) { throw 'Cannot read the workspace package version.' }
  $version
}

function Get-EnglishAssistPayloadHash([string]$Path) {
  $stream = [IO.File]::OpenRead($Path)
  try {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
  } finally { $stream.Dispose() }
}

function Get-EnglishAssistPayloadSpec {
  # Source paths are used only by the explicit production freeze. Repack never
  # copies a native source; its optional refresh is limited to kind=script.
  $rows = @(
    @('retype_ime.dll','target/release/retype_ime.dll','tip-x64','x64'),
    @('retype-diag.exe','target/release/retype-diag.exe','helper','x64'),
    @('settings/retype.exe','target/release/retype-settings-egui.exe','settings','x64'),
    @('retype-learning-host.exe','target/release/retype-learning-host.exe','helper','learning'),
    @('retype-ai-host.exe','target/release/retype-ai-host.exe','helper','ai'),
    @('x86/retype_ime.dll','target/i686-pc-windows-msvc/release/retype_ime.dll','tip-x86','x86'),
    @('dictionaries/english-e1098fbaa665d52788f6ed1fde2a00eb8145e459c41583f5e59f4678db7619bc.bin','dist/english-dictionaries/english-e1098fbaa665d52788f6ed1fde2a00eb8145e459c41583f5e59f4678db7619bc.bin','data','support'),
    @('dictionaries/english-lexicon-v1.bin','dist/english-dictionaries/english-lexicon-v1.bin','data','support'),
    @('dictionaries/english-lexicon-v1.json','dist/english-dictionaries/english-lexicon-v1.json','data','support'),
    @('retype.ico','assets/retype.ico','asset','support'),
    @('LICENSE','LICENSE','license','support'),
    @('NOTICE.txt','NOTICE-English-Assist.txt','license','support'),
    @('LICENSE-Google-Ngram','data/dict/raw/english/LICENSE-Google-Ngram','license','support'),
    @('LICENSE-SCOWL','data/dict/raw/english/LICENSE-SCOWL','license','support'),
    @('LICENSE-SymSpell','data/dict/raw/english/LICENSE-SymSpell','license','support'),
    @('LICENSE-ECDICT','data/dict/raw/english-assist/LICENSE-ECDICT','license','support'),
    @('LICENSE-CC-CEDICT','data/dict/raw/english-assist/LICENSE-CC-CEDICT','license','support'),
    @('NOTICE-English-Lexicon.txt','data/dict/raw/english-assist/NOTICE-English-Lexicon.txt','license','support'),
    @('english-lexicon-sources.json','data/dict/raw/english-assist/SOURCES.json','data','support')
  )
  foreach ($name in @('user-profile.ps1','cleanup-english-assist.ps1','install-state.ps1','manual-words-access.ps1','powershell-environment.ps1')) {
    $rows += ,@($name,('platforms/windows/installer/' + $name),'script','support')
  }
  foreach ($row in $rows) { [pscustomobject]@{path=$row[0]; source=$row[1]; kind=$row[2]; group=$row[3]} }
}

function Assert-EnglishAssistPayloadFile([string]$Root, [string]$Relative) {
  $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\')
  $path = [IO.Path]::GetFullPath((Join-Path $rootPath $Relative))
  if (-not $path.StartsWith($rootPath + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Payload path escaped its root.' }
  $cursor = $path
  while ($cursor.Length -ge $rootPath.Length) {
    if (([IO.File]::GetAttributes($cursor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Payload contains a reparse point.' }
    if ($cursor -eq $rootPath) { break }
    $cursor = [IO.Path]::GetDirectoryName($cursor)
  }
  if (-not [IO.File]::Exists($path)) { throw "Frozen payload file is missing: $Relative" }
  $path
}

function Copy-EnglishAssistProductionGroup([string]$RepoRoot, [string]$StageRoot, [string]$Group) {
  if ($Group -notin @('x64','learning','ai','x86','support')) { throw 'Unknown production artifact group.' }
  foreach ($entry in @(Get-EnglishAssistPayloadSpec | Where-Object group -eq $Group)) {
    $source = Join-Path $RepoRoot $entry.source
    $expected = Get-EnglishAssistPayloadHash $source
    $destination = Join-Path $StageRoot $entry.path
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
    [IO.File]::Copy($source, $destination, $true)
    if ((Get-EnglishAssistPayloadHash $destination) -ne $expected) { throw "Production file changed during freeze: $($entry.path)" }
  }
}

function Get-EnglishAssistPayloadRecords([string]$PayloadRoot) {
  foreach ($entry in @(Get-EnglishAssistPayloadSpec)) {
    $path = Assert-EnglishAssistPayloadFile $PayloadRoot $entry.path
    [ordered]@{path=$entry.path; kind=$entry.kind; bytes=([IO.FileInfo]$path).Length; sha256=(Get-EnglishAssistPayloadHash $path)}
  }
}

function Write-EnglishAssistPayloadManifest([string]$PayloadRoot, [object]$Manifest) {
  [IO.File]::WriteAllText((Join-Path $PayloadRoot 'build.json'),
    (($Manifest | ConvertTo-Json -Depth 12) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
}

function Complete-EnglishAssistPayload([string]$RepoRoot, [string]$PayloadRoot, [string]$Version) {
  if ((Get-EnglishAssistSourceVersion $RepoRoot) -ne $Version) { throw 'Requested build version differs from the workspace source version.' }
  $metadata = [ordered]@{
    schema=2; product='English Assist'; version=$Version; sourceVersion=$Version
    upstreamCommit='902e35e06ee4f744320cf995c39b1f64693034cf'
    tipClsid='{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'
    englishProfile='{F395A724-DC88-4E75-B3D4-9063A552B8F4}'; languageId='0409'
    lexicon=(ConvertFrom-Json ([IO.File]::ReadAllText((Join-Path $PayloadRoot 'dictionaries/english-lexicon-v1.json'))))
    metadataCompilerSha256=(Get-EnglishAssistPayloadHash (Join-Path $RepoRoot 'tools/english-assist/build-lexicon.py'))
    EnglishContinuous=(ConvertFrom-Json ([IO.File]::ReadAllText((Join-Path $RepoRoot 'core/english/assets/bigrams.json'))))
    artifacts=@(Get-EnglishAssistPayloadRecords $PayloadRoot)
  }
  Write-EnglishAssistPayloadManifest $PayloadRoot $metadata
  $null = Assert-EnglishAssistFrozenPayload $PayloadRoot $Version
}

function Assert-EnglishAssistFrozenPayload([string]$PayloadRoot, [string]$Version) {
  $manifestPath = Assert-EnglishAssistPayloadFile $PayloadRoot 'build.json'
  $metadata = ConvertFrom-Json ([IO.File]::ReadAllText($manifestPath))
  if ($metadata.schema -ne 2 -or $metadata.product -ne 'English Assist' -or
      $metadata.version -ne $Version -or $metadata.sourceVersion -ne $Version -or
      $metadata.tipClsid -ne '{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}' -or
      $metadata.englishProfile -ne '{F395A724-DC88-4E75-B3D4-9063A552B8F4}' -or $metadata.languageId -ne '0409') {
    throw 'Frozen payload identity/source version is missing or incompatible; run the full production build.'
  }
  $spec = @(Get-EnglishAssistPayloadSpec)
  if (@($metadata.artifacts).Count -ne $spec.Count) { throw 'Frozen payload manifest has an incomplete or duplicate artifact list.' }
  foreach ($entry in $spec) {
    $record = @($metadata.artifacts | Where-Object path -CEQ $entry.path)
    $path = Assert-EnglishAssistPayloadFile $PayloadRoot $entry.path
    if ($record.Count -ne 1 -or $record[0].kind -ne $entry.kind -or
        $record[0].sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        ([IO.FileInfo]$path).Length -ne $record[0].bytes -or
        (Get-EnglishAssistPayloadHash $path) -ne $record[0].sha256) {
      throw "Frozen payload size/hash mismatch: $($entry.path)"
    }
  }
  $lexicon = $metadata.lexicon
  $image = @($metadata.artifacts | Where-Object path -CEQ 'dictionaries/english-lexicon-v1.bin')[0]
  if ($lexicon.schema -ne 1 -or $lexicon.format -ne 'RTENLX01' -or
      $lexicon.sha256 -ne $image.sha256 -or $lexicon.bytes -ne $image.bytes -or
      $metadata.metadataCompilerSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Frozen Python metadata digest is inconsistent.' }
  # The bigram image is embedded in the production TIP, not another runtime file.
  # Preserve its reviewed input/compiler/image receipt; native artifacts above
  # remain the authority for the exact DLL shipped. Older 0.2 freezes may lack it.
  $continuous = $metadata.EnglishContinuous
  if ($continuous -or [version]$Version -ge [version]'0.3.0') {
    if ($continuous.schema -ne 1 -or $continuous.format -ne 'RTENBG01' -or
        $continuous.file -ne 'bigrams.bin' -or $continuous.bytes -le 32 -or
        $continuous.sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $continuous.compiler_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $continuous.source_manifest_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $continuous.counts.words -le 0 -or $continuous.counts.pairs -le 0 -or
        $continuous.source.repository -cne 'https://github.com/wolfgarbe/SymSpell' -or
        $continuous.source.commit -cnotmatch '^[a-f0-9]{40}$' -or
        $continuous.source.path -cne 'SymSpell/frequency_bigramdictionary_en_243_342.txt' -or
        $continuous.source.sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $continuous.source.uncompressedSha256 -cnotmatch '^[a-f0-9]{64}$') {
      throw 'Frozen embedded English bigram receipt is missing or inconsistent.'
    }
  }
  $metadata
}

function Update-EnglishAssistFrozenScripts([string]$RepoRoot, [string]$PayloadRoot, [string]$Version) {
  if ((Get-EnglishAssistSourceVersion $RepoRoot) -ne $Version) { throw 'Source version changed; a new production build is required.' }
  # Validate before copying anything: a corrupted/overwritten native payload
  # cannot be blessed by regenerating its manifest during a script-only repack.
  $metadata = Assert-EnglishAssistFrozenPayload $PayloadRoot $Version
  foreach ($entry in @(Get-EnglishAssistPayloadSpec | Where-Object kind -eq 'script')) {
    [IO.File]::Copy((Join-Path $RepoRoot $entry.source), (Join-Path $PayloadRoot $entry.path), $true)
  }
  $records = @(Get-EnglishAssistPayloadRecords $PayloadRoot)
  foreach ($record in $records) {
    if ($record.kind -eq 'script') { continue }
    $before = @($metadata.artifacts | Where-Object path -CEQ $record.path)[0]
    if ($before.sha256 -ne $record.sha256 -or $before.bytes -ne $record.bytes) { throw 'A non-script file changed during script refresh.' }
  }
  $metadata.artifacts = $records
  Write-EnglishAssistPayloadManifest $PayloadRoot $metadata
  $null = Assert-EnglishAssistFrozenPayload $PayloadRoot $Version
}

function Publish-EnglishAssistFrozenPayload([string]$RepoRoot, [string]$StageRoot, [string]$Version) {
  $dist = [IO.Path]::GetFullPath((Join-Path $RepoRoot 'dist')).TrimEnd('\')
  $stage = [IO.Path]::GetFullPath($StageRoot).TrimEnd('\')
  $destination = Join-Path $dist 'english-assist'
  if ([IO.Path]::GetDirectoryName($stage) -ne $dist -or
      [IO.Path]::GetFileName($stage) -notmatch '^\.english-assist-freeze-[a-f0-9]{32}$') { throw 'Unsafe payload staging path.' }
  if (([IO.File]::GetAttributes($dist) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Build output directory is a reparse point.' }
  $null = Assert-EnglishAssistFrozenPayload $stage $Version
  $backup = $null
  if ([IO.Directory]::Exists($destination)) {
    if (([IO.File]::GetAttributes($destination) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Frozen payload destination is a reparse point.' }
    # Preserve the preceding freeze; no recursive delete or process shutdown.
    $backup = Join-Path $dist ('english-assist.previous-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::Move($destination, $backup)
  }
  try { [IO.Directory]::Move($stage, $destination) }
  catch {
    if ($backup -and -not [IO.Directory]::Exists($destination)) { [IO.Directory]::Move($backup, $destination) }
    throw
  }
  $destination
}

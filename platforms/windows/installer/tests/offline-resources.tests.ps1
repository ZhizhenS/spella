# Extract only the pure resource validators. Never execute enrollment, COM or registry code.
$taskDictionaryRoot = Join-Path (Join-Path $PSScriptRoot '..\..\..\..') 'dist\english-dictionaries'
$ErrorActionPreference = 'Stop'
$taskSource = Join-Path $PSScriptRoot '..\user-profile.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($taskSource, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Profile script did not parse.' }
foreach ($name in @('Get-EnglishAssistSha256','Test-EnglishAssistOfflineLexicon')) {
  $function = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
  }, $true)
  if (-not $function) { throw "Offline resource function $name was not found." }
  Invoke-Expression $function.Extent.Text
}
# Simulate Setup's missing module function. Any accidental dependency fails loudly.
function Get-FileHash { throw [Management.Automation.CommandNotFoundException]::new('Get-FileHash unavailable in simulated Setup environment') }
$taskRoot = Join-Path ([IO.Path]::GetTempPath()) ('EnglishAssist-resource-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($taskRoot) | Out-Null
$taskImage = Join-Path $taskRoot 'english-lexicon-v1.bin'
$taskManifest = Join-Path $taskRoot 'english-lexicon-v1.json'
$script:Passed = 0
function Reset-ResourceFixture {
  [IO.File]::WriteAllBytes($taskImage, [Text.Encoding]::ASCII.GetBytes('RTENLX01' + ('x' * 24)))
  $script:Manifest = [ordered]@{schema=1;format='RTENLX01';file='english-lexicon-v1.bin';bytes=32;sha256='a92eb5b78b7e31a1c066eb887cc839f178140762697d164224cb8bb03fcbe75f'}
  Save-ResourceManifest
}
function Save-ResourceManifest {
  [IO.File]::WriteAllText($taskManifest, ($script:Manifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
}
function Assert-Rejected([scriptblock]$Prepare, [string]$Name) {
  Reset-ResourceFixture
  & $Prepare
  $rejected = $false
  try { $null = Test-EnglishAssistOfflineLexicon $taskRoot } catch { $rejected = $true }
  if (-not $rejected) { throw "$Name was accepted" }
  $script:Passed++
}
try {
  $unavailable = $false
  try { Get-FileHash -LiteralPath $taskImage -Algorithm SHA256 } catch [Management.Automation.CommandNotFoundException] { $unavailable = $true }
  if (-not $unavailable) { throw 'Module unavailability simulation failed.' }
  $script:Passed++
  [IO.File]::WriteAllBytes($taskImage, [Text.Encoding]::ASCII.GetBytes('abc'))
  if ((Get-EnglishAssistSha256 $taskImage) -cne 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad') { throw 'SHA256 known vector failed.' }
  $script:Passed++
  [IO.File]::WriteAllBytes($taskImage, [byte[]]@())
  if ((Get-EnglishAssistSha256 $taskImage) -cne 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855') { throw 'Empty SHA256 vector failed.' }
  $script:Passed++
  # Reopening exclusively proves the streaming helper disposed its file handle.
  $exclusive = [IO.File]::Open($taskImage, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
  $exclusive.Dispose(); $script:Passed++
  Remove-Item -LiteralPath $taskImage
  if (Test-EnglishAssistOfflineLexicon $taskRoot) { throw 'Old package without metadata was rejected.' }
  $script:Passed++
  Reset-ResourceFixture
  if (-not (Test-EnglishAssistOfflineLexicon $taskRoot)) { throw 'Valid offline resource was rejected.' }
  $script:Passed++
  Assert-Rejected { Remove-Item -LiteralPath $taskImage } 'Missing image'
  Assert-Rejected { Remove-Item -LiteralPath $taskManifest } 'Missing manifest'
  Assert-Rejected { [IO.File]::WriteAllText($taskManifest, '{') } 'Invalid JSON'
  Assert-Rejected { $script:Manifest.schema = 2; Save-ResourceManifest } 'Unknown schema'
  Assert-Rejected { $script:Manifest.format = 'RTENGL01'; Save-ResourceManifest } 'Wrong format'
  Assert-Rejected { $script:Manifest.file = '..\other.bin'; Save-ResourceManifest } 'Path outside fixed image'
  Assert-Rejected { $script:Manifest.sha256 = 'z' * 64; Save-ResourceManifest } 'Invalid digest'
  Assert-Rejected { $script:Manifest.sha256 = '0' * 64; Save-ResourceManifest } 'Digest mismatch'
  Assert-Rejected { $script:Manifest.bytes = 67108865; Save-ResourceManifest } 'Unbounded image size'
  Assert-Rejected { $script:Manifest.bytes = 31; Save-ResourceManifest } 'Size mismatch'
  Assert-Rejected { [IO.File]::WriteAllBytes($taskImage, [Text.Encoding]::ASCII.GetBytes('RTENGL01' + ('x' * 24))); $script:Manifest.sha256 = '199f1c0a56dd057001679ec42cc5e5f354876a91ecdeaabb26196d625bba931f'; Save-ResourceManifest } 'Hash-valid wrong magic'
  Assert-Rejected { Remove-Item -LiteralPath $taskImage; [IO.Directory]::CreateDirectory($taskImage) | Out-Null } 'Directory in place of image'
  Remove-Item -LiteralPath $taskImage -Force
  $englishHash = 'e1098fbaa665d52788f6ed1fde2a00eb8145e459c41583f5e59f4678db7619bc'
  if ((Get-EnglishAssistSha256 (Join-Path $taskDictionaryRoot "english-$englishHash.bin")) -cne $englishHash) { throw 'Real English vocabulary verification failed.' }
  $script:Passed++
  if (-not (Test-EnglishAssistOfflineLexicon $taskDictionaryRoot)) { throw 'Real offline metadata verification failed.' }
  $script:Passed++
  Write-Output "$script:Passed offline resource integrity cases passed for $([IntPtr]::Size * 8)-bit PowerShell with Get-FileHash unavailable, including both real dictionaries; no installed resources or system settings modified."
} finally {
  # These are fixed, direct children of this test's randomly named temp directory.
  if (Test-Path -LiteralPath $taskImage) { Remove-Item -LiteralPath $taskImage -Force }
  if (Test-Path -LiteralPath $taskManifest) { Remove-Item -LiteralPath $taskManifest -Force }
  Remove-Item -LiteralPath $taskRoot -Force
}

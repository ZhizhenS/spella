# Each invocation is an isolated Windows PowerShell child process. System state is read-only.
[CmdletBinding()]
param(
  [ValidateSet('Missing','CoreOnly','Foreign')][string]$Mode = 'Missing',
  [string]$CoreModulePath = 'C:\Program Files\PowerShell\7\Modules'
)
$ErrorActionPreference = 'Stop'
if ($PSVersionTable.PSEdition -ne 'Desktop') { throw 'Run this probe in Windows PowerShell 5.1.' }
$source = [IO.Path]::GetFullPath([IO.Path]::Combine($PSScriptRoot, '..\powershell-environment.ps1'))
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'Profile script did not parse.' }
$definition = $ast.Find({ param($node)
  $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Initialize-EnglishAssistPowerShell'
}, $true)
if (-not $definition) { throw 'PowerShell bootstrap function was not found.' }
. ([scriptblock]::Create($definition.Extent.Text))
$fixture = [IO.Path]::Combine([IO.Path]::GetTempPath(), 'EnglishAssist-bootstrap-' + [Guid]::NewGuid().ToString('N'))
$null = [IO.Directory]::CreateDirectory($fixture)
$vector = [IO.Path]::Combine($fixture, 'abc.txt')
[IO.File]::WriteAllBytes($vector, [Text.Encoding]::ASCII.GetBytes('abc'))
$original = [Environment]::GetEnvironmentVariable('PSModulePath', 'Process')
$userBefore = [Environment]::GetEnvironmentVariable('PSModulePath', 'User')
$machineBefore = [Environment]::GetEnvironmentVariable('PSModulePath', 'Machine')
$script:Passed = 0
try {
  # Only Core commands and .NET are used between clearing modules and bootstrap.
  foreach ($name in @('Microsoft.PowerShell.Security','Microsoft.PowerShell.Utility','Microsoft.PowerShell.Management','International')) {
    foreach ($loaded in @(Microsoft.PowerShell.Core\Get-Module -Name $name)) {
      Microsoft.PowerShell.Core\Remove-Module -ModuleInfo $loaded -Force -ErrorAction Stop
    }
  }
  if ($Mode -eq 'Missing') {
    [Environment]::SetEnvironmentVariable('PSModulePath', $null, 'Process')
  } elseif ($Mode -eq 'CoreOnly') {
    [Environment]::SetEnvironmentVariable('PSModulePath', $CoreModulePath, 'Process')
  } else {
    [Environment]::SetEnvironmentVariable('PSModulePath', $fixture, 'Process')
    foreach ($name in @('Microsoft.PowerShell.Security','Microsoft.PowerShell.Utility')) {
      $directory = [IO.Path]::Combine($fixture, $name)
      $null = [IO.Directory]::CreateDirectory($directory)
      $command = if ($name -eq 'Microsoft.PowerShell.Security') { 'Get-Acl' } else { 'Get-FileHash' }
      $module = [IO.Path]::Combine($directory, $name + '.psm1')
      [IO.File]::WriteAllText($module, ('function ' + $command + ' { throw "foreign-module-proxy" }; Export-ModuleMember -Function ' + $command))
      Microsoft.PowerShell.Core\Import-Module -Name $module -Scope Global -Force -ErrorAction Stop
    }
    # Verify the foreign exports really won command lookup before testing removal.
    $foreignPath = [IO.Path]::Combine([IO.Path]::Combine($fixture,'Microsoft.PowerShell.Security'),'Microsoft.PowerShell.Security.psm1')
    if ((Microsoft.PowerShell.Core\Get-Command Get-Acl).Module.Path -ne $foreignPath) {
      $found = Microsoft.PowerShell.Core\Get-Command Get-Acl
      throw "Foreign module simulation failed: $($found.CommandType), $($found.Module.Path)"
    }
    $script:Passed++
  }
  Initialize-EnglishAssistPowerShell
  $moduleRoot = [IO.Path]::Combine($PSHOME, 'Modules')
  if ($env:PSModulePath.Split([IO.Path]::PathSeparator)[0] -ne $moduleRoot) { throw 'Native module path is not first.' }
  $script:Passed++
  foreach ($pair in @(@('Get-Acl','Microsoft.PowerShell.Security'), @('Get-FileHash','Microsoft.PowerShell.Utility'),
      @('Get-Item','Microsoft.PowerShell.Management'), @('Get-WinUserLanguageList','International'))) {
    $command = Microsoft.PowerShell.Core\Get-Command $pair[0]
    $expectedManifest = [IO.Path]::Combine([IO.Path]::Combine($moduleRoot,$pair[1]),$pair[1]+'.psd1')
    if ($command.Module.Path -ne $expectedManifest) { throw "Foreign command survived bootstrap: $($pair[0])" }
    $script:Passed++
  }
  # These exercise the exact command/providers which previously failed in Setup.
  $key = 'HKCU:\Software\EnglishAssist'
  if (-not (Test-Path -LiteralPath $key)) { $key = 'HKCU:\Software' }
  # Windows PowerShell 5.1's registry provider requires the production -Path
  # form here; its Get-Acl -LiteralPath path resolution is a separate limitation.
  $acl = Get-Acl -Path $key
  if ($acl -isnot [Security.AccessControl.RegistrySecurity]) { throw 'Native registry Get-Acl failed.' }
  $script:Passed++
  if ((Get-FileHash -LiteralPath $vector -Algorithm SHA256).Hash -ne 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad') { throw 'Native Get-FileHash failed.' }
  $script:Passed++
  $languages = @(Get-WinUserLanguageList)
  if ($languages.Count -lt 1) { throw 'International read-only query failed.' }
  $script:Passed++
  $null = Get-WinDefaultInputMethodOverride
  $script:Passed++
  if ([Environment]::GetEnvironmentVariable('PSModulePath','User') -ne $userBefore -or
      [Environment]::GetEnvironmentVariable('PSModulePath','Machine') -ne $machineBefore) { throw 'Persistent module environment changed.' }
  $script:Passed++
  Write-Output "$script:Passed Windows PowerShell bootstrap cases passed ($Mode, $([IntPtr]::Size * 8)-bit); native ACL/hash/International reads worked; no system settings changed."
} finally {
  [Environment]::SetEnvironmentVariable('PSModulePath', $original, 'Process')
  foreach ($name in @('Microsoft.PowerShell.Security','Microsoft.PowerShell.Utility')) {
    $directory = [IO.Path]::Combine($fixture, $name)
    $file = [IO.Path]::Combine($directory, $name + '.psm1')
    if ([IO.File]::Exists($file)) { [IO.File]::Delete($file) }
    if ([IO.Directory]::Exists($directory)) { [IO.Directory]::Delete($directory) }
  }
  if ([IO.File]::Exists($vector)) { [IO.File]::Delete($vector) }
  [IO.Directory]::Delete($fixture)
}

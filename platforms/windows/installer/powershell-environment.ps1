# Definitions only. Initialization changes only this Windows PowerShell child process.
function Initialize-EnglishAssistPowerShell {
  if ($PSVersionTable.PSEdition -ne 'Desktop' -or $PSVersionTable.PSVersion -lt [version]'5.1') {
    throw 'English Assist user setup requires Windows PowerShell 5.1.'
  }
  $moduleRoot = [IO.Path]::Combine($PSHOME, 'Modules')
  # Setup can inherit PowerShell 7's module search paths from its parent app.
  # This changes only the current child process, never user/machine environment.
  $inherited = [Environment]::GetEnvironmentVariable('PSModulePath', 'Process')
  $env:PSModulePath = $moduleRoot
  if (-not [string]::IsNullOrWhiteSpace($inherited)) {
    $env:PSModulePath += [IO.Path]::PathSeparator + $inherited
  }
  foreach ($name in @('Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility',
      'Microsoft.PowerShell.Security', 'International')) {
    $directory = [IO.Path]::Combine($moduleRoot, $name)
    $manifest = [IO.Path]::Combine($directory, $name + '.psd1')
    if (-not [IO.File]::Exists($manifest)) { throw "Required Windows PowerShell module is missing: $name" }
    foreach ($loaded in @(Microsoft.PowerShell.Core\Get-Module -Name $name)) {
      if ($loaded.Path -ne $manifest) {
        # Importing a native cmdlet alone cannot outrank a foreign module's function.
        Microsoft.PowerShell.Core\Remove-Module -ModuleInfo $loaded -Force -ErrorAction Stop
      }
    }
    Microsoft.PowerShell.Core\Import-Module -Name $manifest -Scope Global -Force -ErrorAction Stop
  }
}

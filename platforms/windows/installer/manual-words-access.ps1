# Functions only: importing does not create directories, alter ACLs or publish settings.
function Get-EnglishAssistManualWordsAttributes([string]$Path) {
  try { return [int][IO.File]::GetAttributes($Path) }
  catch [IO.FileNotFoundException] { return 0 }
  catch [IO.DirectoryNotFoundException] { return 0 }
}

function Get-EnglishAssistManualWordsLayout(
  [string]$LocalAppData,
  [scriptblock]$ReadAttributes = { param($Path) Get-EnglishAssistManualWordsAttributes $Path }
) {
  if ([string]::IsNullOrWhiteSpace($LocalAppData) -or $LocalAppData -notmatch '^[A-Za-z]:[\\/]') {
    throw 'Personal dictionary requires an absolute local application-data path.'
  }
  $local = [IO.Path]::GetFullPath($LocalAppData).TrimEnd('\')
  if ($local -match '^[A-Za-z]:$') { throw 'Personal dictionary cannot use a drive root as application data.' }
  # Check every existing ancestor before creating a directory or following a path.
  # GetAttributes reads the link itself, including a dangling link on Windows.
  $cursor = $local
  while ($cursor) {
    $attributes = & $ReadAttributes $cursor
    if (($attributes -band 0x400) -ne 0 -or ($attributes -band 0x10) -eq 0) {
      throw 'Personal dictionary application-data path is missing, not a directory, or contains a reparse point.'
    }
    $cursor = [IO.Path]::GetDirectoryName($cursor)
  }
  $root = Join-Path $local 'EnglishAssist'
  $words = Join-Path $root 'words'
  foreach ($directory in @($root, $words)) {
    $attributes = & $ReadAttributes $directory
    if (($attributes -band 0x400) -ne 0 -or ($attributes -ne 0 -and ($attributes -band 0x10) -eq 0)) {
      throw 'Personal dictionary product/words directory contains a reparse point or is not a directory.'
    }
  }
  $file = Join-Path $words 'manual.json'
  $attributes = & $ReadAttributes $file
  if (($attributes -band 0x410) -ne 0) {
    throw 'Personal dictionary manual.json must be a regular file, never a directory or reparse point.'
  }
  [pscustomobject]@{ Root=$root; Words=$words; File=$file }
}

function Get-EnglishAssistManualWordsChildren(
  [string]$Words,
  [scriptblock]$ReadChildren = { param($Path) [IO.Directory]::EnumerateFileSystemEntries($Path) },
  [scriptblock]$ReadAttributes = { param($Path) Get-EnglishAssistManualWordsAttributes $Path }
) {
  # The manual store has flat JSON/backup/temp files. Refuse nested directories
  # and links before ACL propagation, so no external tree can become readable.
  foreach ($child in (& $ReadChildren $Words)) {
    $path = [IO.Path]::GetFullPath($child)
    if ([IO.Path]::GetDirectoryName($path) -ne $Words -or
        ((& $ReadAttributes $path) -band 0x410) -ne 0) {
      throw 'Personal dictionary contains a directory, reparse point or entry outside its words directory.'
    }
    $path
  }
}

function Get-EnglishAssistManualWordsReadRules {
  $packages = [Security.Principal.SecurityIdentifier]::new('S-1-15-2-1')
  # RX/Traverse applies to directories; files inherit Read, without ExecuteFile.
  [Security.AccessControl.FileSystemAccessRule]::new($packages,
    [Security.AccessControl.FileSystemRights]::ReadAndExecute,
    [Security.AccessControl.InheritanceFlags]::ContainerInherit,
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow)
  [Security.AccessControl.FileSystemAccessRule]::new($packages,
    [Security.AccessControl.FileSystemRights]::Read,
    [Security.AccessControl.InheritanceFlags]::ObjectInherit,
    [Security.AccessControl.PropagationFlags]::InheritOnly,
    [Security.AccessControl.AccessControlType]::Allow)
}

function Initialize-EnglishAssistManualWordsAccess([string]$LocalAppData) {
  $layout = Get-EnglishAssistManualWordsLayout $LocalAppData
  [IO.Directory]::CreateDirectory($layout.Root) | Out-Null
  $layout = Get-EnglishAssistManualWordsLayout $LocalAppData
  [IO.Directory]::CreateDirectory($layout.Words) | Out-Null
  $layout = Get-EnglishAssistManualWordsLayout $LocalAppData
  $children = @(Get-EnglishAssistManualWordsChildren $layout.Words)
  $rules = @(Get-EnglishAssistManualWordsReadRules)
  $acl = Get-Acl -LiteralPath $layout.Words
  $acl.SetAccessRule($rules[0])
  $acl.AddAccessRule($rules[1])
  Set-Acl -LiteralPath $layout.Words -AclObject $acl
  # Existing JSON/backup files may have disabled inheritance. Preserve their
  # contents and all other identities' permissions while granting only Read.
  foreach ($file in $children) {
    $null = Get-EnglishAssistManualWordsLayout $LocalAppData
    $null = Get-EnglishAssistManualWordsChildren $layout.Words
    $fileAcl = Get-Acl -LiteralPath $file
    $fileAcl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
      [Security.Principal.SecurityIdentifier]::new('S-1-15-2-1'),
      [Security.AccessControl.FileSystemRights]::Read,
      [Security.AccessControl.AccessControlType]::Allow))
    Set-Acl -LiteralPath $file -AclObject $fileAcl
  }
  # Never create/truncate manual.json: settings owns its content and atomic writes.
  $layout.File
}

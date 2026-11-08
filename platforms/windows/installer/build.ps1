# Compatibility entry: this fork builds English Assist only.
[CmdletBinding()]
param([switch]$SkipDict, [switch]$NoTest, [switch]$Installer)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'build-english-assist.ps1') -NoTest:$NoTest -Installer:$Installer
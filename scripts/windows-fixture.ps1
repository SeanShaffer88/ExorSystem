<#
.SYNOPSIS
  Create and change a disposable real file/HKCU fixture for Windows 10/11 release checks.
.DESCRIPTION
  Use only in a disposable VM, from the same normal Windows account as ExorSystem.
  No machine registry, service, task, driver, or installer changes are made. New returns
  an ID, exact Watch scopes and a per-user inventory entry. Only that fixture is removed.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [ValidateSet('New','ChangeA','ChangeB','Verify','LockFile','MakeOversized','Remove')] [string]$Action,
  [ValidatePattern('^[a-f0-9]{32}$')] [string]$Id
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'This fixture needs Windows.' }
if ($Action -eq 'New') {
  if ($Id) { throw 'New allocates its own fixture ID.' }
  $Id = [guid]::NewGuid().ToString('N')
} elseif (-not $Id) { throw 'Pass the ID returned by New.' }

$local = [Environment]::GetFolderPath('LocalApplicationData')
if (-not $local -or -not [IO.Path]::IsPathRooted($local)) { throw 'Cannot find LocalAppData.' }
$base = Join-Path $local 'ExorSystemRoadmap'
$dir = Join-Path $base $Id
$a = Join-Path $dir 'session-a'
$b = Join-Path $dir 'session-b'
$owner = Join-Path $dir '.exor-fixture-id'
$name = "ExorSystem Roadmap $Id"
$registry = "HKCU:\Software\$name"
$arp = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ExorRoadmap-$Id"
$locked = Join-Path $a 'locked.txt'
$exe = Join-Path $dir 'Fixture.exe'

function Assert-PlainDirectory([string]$Path) {
  $item = Get-Item -LiteralPath $Path -Force
  if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "Not a plain fixture directory: $Path"
  }
}
function Write-FixtureFile([string]$Path, [string]$Text) {
  if (Test-Path -LiteralPath $Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Not a plain fixture file: $Path" }
  }
  [IO.File]::WriteAllText($Path, $Text)
}

if ($Action -eq 'New') {
  if (-not (Test-Path -LiteralPath $base)) { [void](New-Item -ItemType Directory -Path $base) }
  Assert-PlainDirectory $base
  foreach ($path in @($dir, $registry, $arp)) {
    if (Test-Path -LiteralPath $path) { throw "Fixture already exists: $path" }
  }
  foreach ($path in @($dir, $a, $b)) { [void](New-Item -ItemType Directory -Path $path) }
  Write-FixtureFile $owner $Id
  Write-FixtureFile (Join-Path $a 'a-change.txt') 'before A'
  Write-FixtureFile (Join-Path $a 'a-gone.txt') 'removed during A'
  Write-FixtureFile $locked 'safe lock test'
  $where = Join-Path ([Environment]::GetFolderPath('System')) 'where.exe'
  Copy-Item -LiteralPath $where -Destination $exe
  [void](New-Item -Path $registry)
  [void](New-Item -Path "$registry\Changed")
  [void](New-ItemProperty -Path "$registry\Changed" -Name 'FixtureValue' -Value 'before A' -PropertyType String)
  [void](New-Item -Path "$registry\Gone")
  [void](New-Item -Path $arp -Force)
  [void](New-ItemProperty -Path $arp -Name 'DisplayName' -Value $name -PropertyType String)
  [void](New-ItemProperty -Path $arp -Name 'InstallLocation' -Value $a -PropertyType String)
  # A real but harmless executable name makes it possible to check the HKCU launch refusal.
  # ExorSystem must not offer a launch; do not run this fixture entry through Windows Settings.
  [void](New-ItemProperty -Path $arp -Name 'UninstallString' -Value ('"' + $where + '" ExorFixtureDoesNotExist') -PropertyType String)
} else {
  foreach ($path in @($base, $dir, $a, $b)) { Assert-PlainDirectory $path }
  $tag = Get-Item -LiteralPath $owner -Force
  if ($tag.PSIsContainer -or ($tag.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      [IO.File]::ReadAllText($tag.FullName) -cne $Id) { throw 'Fixture marker did not match.' }
}

switch ($Action) {
  'ChangeA' {
    Write-FixtureFile (Join-Path $a 'a-change.txt') 'after A'
    Write-FixtureFile (Join-Path $a 'a-created.txt') 'created during A'
    Remove-Item -LiteralPath (Join-Path $a 'a-gone.txt')
    Set-ItemProperty -LiteralPath "$registry\Changed" -Name 'FixtureValue' -Value 'after A'
    Remove-Item -LiteralPath "$registry\Gone"
    [void](New-Item -Path "$registry\Added")
  }
  'ChangeB' {
    # More than one default History page, with no session A names.
    foreach ($i in 1..105) { Write-FixtureFile (Join-Path $b ('b-{0:d3}.txt' -f $i)) "created during B: $i" }
    Write-FixtureFile (Join-Path $b 'b-percent%_literal.txt') 'literal filter'
    [void](New-Item -Path "$registry\Second")
  }
  'Verify' {
    if ([IO.File]::ReadAllText((Join-Path $a 'a-change.txt')) -cne 'after A' -or
        (Test-Path -LiteralPath (Join-Path $a 'a-gone.txt')) -or
        -not (Test-Path -LiteralPath (Join-Path $a 'a-created.txt')) -or
        (Test-Path -LiteralPath "$registry\Gone") -or
        -not (Test-Path -LiteralPath "$registry\Added") -or
        -not (Test-Path -LiteralPath "$registry\Second") -or
        (Get-ItemProperty -LiteralPath "$registry\Changed").FixtureValue -cne 'after A' -or
        @(Get-ChildItem -LiteralPath $b -File).Count -ne 106) { throw 'Unexpected fixture contents; run both Change actions first.' }
  }
  'LockFile' {
    $stream = [IO.File]::Open($locked, 'Open', 'Read', 'Read')
    try { [void](Read-Host "Sharing violation fixture active: $locked. Press Enter to release it") }
    finally { $stream.Dispose() }
  }
  'MakeOversized' {
    $large = "$registry\Oversized"
    [void](New-Item -Path $large)
    foreach ($i in 1..4096) { [void](New-Item -Path "$large\$i") }
  }
  'Remove' {
    # Remove known files only. Never recursively delete filesystem contents: a manual
    # reparse-point test or collected logs may have left extra entries in the fixture.
    foreach ($path in @($exe, $locked, (Join-Path $a 'a-change.txt'), (Join-Path $a 'a-gone.txt'),
        (Join-Path $a 'a-created.txt'), (Join-Path $b 'b-percent%_literal.txt'))) {
      [IO.File]::Delete($path)
    }
    foreach ($i in 1..105) { [IO.File]::Delete((Join-Path $b ('b-{0:d3}.txt' -f $i))) }
    if (@(Get-ChildItem -LiteralPath $a -Force).Count -or @(Get-ChildItem -LiteralPath $b -Force).Count) {
      throw 'Extra files or links remain. Inspect the fixture and remove those explicitly, or revert the VM.'
    }
    foreach ($key in @($arp, $registry)) {
      if (Test-Path -LiteralPath $key) { Remove-Item -LiteralPath $key -Recurse }
    }
    foreach ($path in @($a, $b)) { [IO.Directory]::Delete($path, $false) }
    [IO.File]::Delete($owner)
    if (@(Get-ChildItem -LiteralPath $dir -Force).Count -eq 0) { [IO.Directory]::Delete($dir, $false) }
    return
  }
}

[pscustomobject]@{
  id = $Id
  display_name = $name
  folder_a = $a
  folder_b = $b
  registry_a = "HKCU\Software\$name"
  registry_b = "HKCU\Software\$name\Second"
  oversized = "HKCU\Software\$name\Oversized"
  portable_exe = $exe
  locked_path = $locked
}

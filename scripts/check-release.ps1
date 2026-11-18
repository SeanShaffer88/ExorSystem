<#
.SYNOPSIS
  Verify the three Windows artifacts without running an installer. Requires Windows SDK mt.exe.
.DESCRIPTION
  Writes PACKAGE-CHECKS.json with the commit, hashes, package metadata, and what was NOT tested.
  Run from the checkout that built the candidate. These checks do not install or launch the desktop.
#>
[CmdletBinding()]
param([string]$ArtifactDir = (Join-Path $PSScriptRoot '..\dist'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Run this check on Windows.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$config = Get-Content -Raw (Join-Path $root 'gui\src-tauri\tauri.conf.json') | ConvertFrom-Json
$dir = (Resolve-Path -LiteralPath $ArtifactDir).Path
$prefix = "ExorSystem-v$($config.version)-x64"
$names = @("$prefix-Setup.exe", "$prefix.msi", "$prefix-Portable.zip")
$commit = git -C $root rev-parse HEAD
if ($LASTEXITCODE) { throw 'Could not determine the source commit' }

$checks = @{}
foreach ($line in Get-Content -LiteralPath (Join-Path $dir 'SHA256SUMS.txt')) {
  if ($line -notmatch '^([0-9a-f]{64})  ([^/\\]+)$') { throw 'Invalid SHA256SUMS.txt line' }
  $expectedHash = $Matches[1]
  $name = $Matches[2]
  if (-not ($names -ccontains $name) -or $checks.ContainsKey($name)) { throw "Unexpected or duplicate artifact: $name" }
  $file = Get-Item -LiteralPath (Join-Path $dir $name)
  $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($hash -ne $expectedHash) { throw "SHA-256 mismatch: $name" }
  $checks[$name] = [ordered]@{ name = $name; sha256 = $hash; bytes = $file.Length }
}
if ($checks.Count -ne $names.Count) { throw 'SHA256SUMS.txt must cover all three artifacts' }

function Read-PeMachine([string]$Path) {
  $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
  try {
    if ($reader.BaseStream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Not a PE file: $Path" }
    $reader.BaseStream.Position = 0x3c
    $header = $reader.ReadUInt32()
    if ($header -lt 64 -or $header -gt ($reader.BaseStream.Length - 6)) { throw "Invalid PE offset: $Path" }
    $reader.BaseStream.Position = $header
    if ($reader.ReadUInt32() -ne 0x4550) { throw "Missing PE signature: $Path" }
    return $reader.ReadUInt16()
  } finally { $reader.Dispose() }
}

# The NSIS launcher may be a 32-bit stub even for an x64 application payload.
$checks[$names[0]].pe_machine = ('0x{0:x4}' -f (Read-PeMachine (Join-Path $dir $names[0])))
# Read the MSI Template summary property (PID_TEMPLATE = 7) through Windows Installer.
# Update count zero opens it read-only, without installing the package.
if (-not ('ExorMsiMetadata' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class ExorMsiMetadata {
    [DllImport("msi.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern uint MsiGetSummaryInformationW(uint database, string path, uint updates, out uint summary);

    [DllImport("msi.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern uint MsiSummaryInfoGetPropertyW(uint summary, uint property, out uint kind, out int number,
        out System.Runtime.InteropServices.ComTypes.FILETIME time, StringBuilder value, ref uint size);

    [DllImport("msi.dll", ExactSpelling = true)]
    private static extern uint MsiCloseHandle(uint handle);

    public static string ReadTemplate(string path) {
        uint summary;
        uint error = MsiGetSummaryInformationW(0, path, 0, out summary);
        if (error != 0) throw new Win32Exception((int)error);
        try {
            var text = new StringBuilder(1024);
            uint size = (uint)text.Capacity;
            uint kind;
            int number;
            System.Runtime.InteropServices.ComTypes.FILETIME time;
            error = MsiSummaryInfoGetPropertyW(summary, 7, out kind, out number, out time, text, ref size);
            if (error != 0) throw new Win32Exception((int)error);
            return text.ToString();
        } finally {
            MsiCloseHandle(summary);
        }
    }
}
'@
}
$template = [ExorMsiMetadata]::ReadTemplate((Join-Path $dir $names[1]))
if ($template -notmatch '^x64;') { throw "Expected an x64 MSI, got '$template'" }
$checks[$names[1]].msi_template = $template

Add-Type -AssemblyName System.IO.Compression.FileSystem
$stage = Join-Path ([IO.Path]::GetTempPath()) ('exor-package-check-' + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $stage)
try {
  $expected = @('ExorSystem.exe', 'README.md', 'LICENSE', 'CHANGELOG.md', 'CREDITS.md')
  $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $dir $names[2]))
  try {
    $entries = @($zip.Entries | ForEach-Object FullName)
    if ($entries.Count -ne $expected.Count -or @(Compare-Object $expected $entries -CaseSensitive).Count) {
      throw 'The portable archive does not have the expected files at its root'
    }
    $exe = Join-Path $stage 'ExorSystem.exe'
    [IO.Compression.ZipFileExtensions]::ExtractToFile($zip.GetEntry('ExorSystem.exe'), $exe)
  } finally { $zip.Dispose() }
  if ((Read-PeMachine $exe) -ne 0x8664) { throw 'The portable program is not x64' }

  $mt = Get-Command mt.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source
  if (-not $mt) {
    $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $mt = Get-ChildItem -Directory -Path $sdk | Sort-Object Name -Descending | ForEach-Object {
      Join-Path $_.FullName 'x64\mt.exe'
    } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
  }
  if (-not $mt) { throw 'Windows SDK mt.exe is needed to inspect the actual embedded manifest' }
  $manifestPath = Join-Path $stage 'ExorSystem.manifest'
  & $mt -nologo "-inputresource:$exe;#1" "-out:$manifestPath"
  if ($LASTEXITCODE) { throw 'Could not extract the executable manifest' }
  [xml]$manifest = Get-Content -Raw -LiteralPath $manifestPath
  $level = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
  if (-not $level -or $level.GetAttribute('level') -cne 'asInvoker' -or $level.GetAttribute('uiAccess') -cne 'false') {
    throw 'The desktop executable must run asInvoker with uiAccess=false'
  }
  $dpi = $manifest.SelectSingleNode("//*[local-name()='dpiAwareness']")
  if (-not $dpi -or $dpi.InnerText -cne 'PerMonitorV2') { throw 'Expected PerMonitorV2 DPI awareness' }
  $checks[$names[2]].pe_machine = '0x8664'
  $checks[$names[2]].execution_level = 'asInvoker'
  $checks[$names[2]].dpi_awareness = $dpi.InnerText
  $checks[$names[2]].entries = $entries
} finally { Remove-Item -LiteralPath $stage -Force -Recurse }

[ordered]@{
  kind = 'static-package-checks-only'
  commit = $commit
  app_version = $config.version
  checked_at_utc = [datetime]::UtcNow.ToString('o')
  runner_os = [Environment]::OSVersion.VersionString
  workflow_run = $env:GITHUB_RUN_ID
  artifacts = @($names | ForEach-Object { $checks[$_] })
  not_tested = @('Installation, upgrade and uninstall', 'Desktop, UAC and WebView2', 'Interactive Watch, History and cleanup', 'Accessibility and multi-monitor behavior')
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $dir 'PACKAGE-CHECKS.json') -Encoding utf8
Write-Host 'Static package checks passed. No Windows client install or launch was tested.'

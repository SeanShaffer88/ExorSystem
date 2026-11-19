#requires -Version 7.2
<#
.SYNOPSIS
  Observe the first launch and clean exit of a verified ExorSystem desktop on Windows 10/11 x64.
.DESCRIPTION
  Use a disposable VM and normal account with no existing ExorSystem profile. Manually extract
  or install the official package before the run, without launching the app. The runner checks
  the published asset digests and the selected executable, then observes an operator launch
  and close. It does not execute installers, extract binaries, start/kill the app, or delete
  data. Leave Windows security controls in place. A responding window is not a WebView2 UI pass.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$ArtifactDir,
  [Parameter(Mandatory)] [string]$ReleaseInfo,
  [Parameter(Mandatory)] [string]$Tag,
  [Parameter(Mandatory)] [string]$EvidenceDir,
  [Parameter(Mandatory)] [ValidateSet('Portable', 'NSIS', 'MSI')] [string]$Format,
  [Parameter(Mandatory)] [string]$DesktopExe,
  [switch]$DisposableClient,
  [ValidateRange(10, 120)] [int]$StartupTimeoutSeconds = 60,
  [ValidateRange(10, 180)] [int]$CloseTimeoutSeconds = 90
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not $IsWindows) { throw 'Use the static verifier on this OS. Launch smoke requires a Windows client.' }
if (-not $DisposableClient) { throw 'Repeat only in a disposable VM/account and pass -DisposableClient.' }

function Get-LocalFullPath([string]$Path) {
  # Resolve relative paths against PowerShell's location, not the .NET process's working directory.
  $full = [IO.Path]::GetFullPath($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path))
  if ($full -notmatch '^[A-Za-z]:\\' -or $full.Substring(2).Contains(':') -or
      [IO.DriveInfo]::new($full.Substring(0, 3)).DriveType -ne 'Fixed') {
    throw 'Use an ordinary path on a local fixed drive.'
  }
  return [IO.Path]::TrimEndingDirectorySeparator($full)
}

function Assert-PlainPath([string]$Path, [bool]$Directory) {
  $full = Get-LocalFullPath $Path
  $current = $full
  $wantDirectory = $Directory
  while ($current) {
    $item = Get-Item -LiteralPath $current -Force
    if ($item.PSIsContainer -ne $wantDirectory -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      throw 'A smoke-test path is redirected or is not the expected file or directory type.'
    }
    $parent = [IO.Directory]::GetParent($current)
    $current = if ($null -ne $parent) { $parent.FullName } else { $null }
    $wantDirectory = $true
  }
  return $full
}

function Assert-Absent([string]$Path, [string]$Reason) {
  # Enumeration sees even a dangling reparse point; access failure must not count as absence.
  $entries = [IO.Directory]::GetFileSystemEntries([IO.Path]::GetDirectoryName($Path), [IO.Path]::GetFileName($Path))
  if ($entries.Length -gt 0) { throw $Reason }
}

function Test-InDirectory([string]$Path, [string]$Root) {
  $rootName = $Root.TrimEnd('\')
  return $Path.Equals($rootName, [StringComparison]::OrdinalIgnoreCase) -or
    $Path.StartsWith($rootName + '\', [StringComparison]::OrdinalIgnoreCase)
}

function Write-NewJson([string]$Path, $Value) {
  $stream = [IO.File]::Open($Path, 'CreateNew', 'Write', 'None')
  try {
    $json = $Value | ConvertTo-Json -Depth 10
    $data = [Text.UTF8Encoding]::new($false).GetBytes($json + "`n")
    $stream.Write($data, 0, $data.Length)
  } finally { $stream.Dispose() }
}

function Assert-Binary([string]$Path, $Expected) {
  $null = Assert-PlainPath $Path $false
  if ((Get-Item -LiteralPath $Path).Length -ne $Expected.bytes -or
      (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Expected.sha256) {
    throw 'The desktop executable does not match the verified portable binary.'
  }
}

$os = Get-CimInstance -ClassName Win32_OperatingSystem -Property Version, BuildNumber, ProductType -OperationTimeoutSec 5
$arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ($os.ProductType -ne 1 -or $os.Version -notmatch '^10\.' -or [int]$os.BuildNumber -lt 10240 -or $arch -ne 'X64' -or -not [Environment]::UserInteractive) {
  throw 'Use an interactive Windows 10/11 x64 client, not a hosted/server runner.'
}
if (-not [Environment]::Is64BitProcess) { throw 'Use x64 PowerShell to observe the x64 desktop process.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Use a normal, unelevated test account.' }
} finally { $identity.Dispose() }

$packages = Assert-PlainPath $ArtifactDir $true
$metadata = Assert-PlainPath $ReleaseInfo $false
$local = Assert-PlainPath ([Environment]::GetFolderPath('LocalApplicationData')) $true
$profile = Join-Path $local 'ExorSystem'
$freshMessage = 'This account already has an ExorSystem profile. Use a fresh test account or revert the VM; do not delete the profile.'
Assert-Absent $profile $freshMessage
$exe = Assert-PlainPath $DesktopExe $false
if ([IO.Path]::GetFileName($exe) -notin @('ExorSystem.exe', 'exorsystem-gui.exe')) {
  throw 'Select the extracted or installed desktop executable, not a shortcut or installer.'
}
$out = Get-LocalFullPath $EvidenceDir
if ($out -eq [IO.Path]::GetPathRoot($out)) { throw 'Choose a new evidence folder, not a drive root.' }
if ((Test-InDirectory $out $packages) -or (Test-InDirectory $out $profile)) {
  throw 'Keep evidence outside both the package directory and the ExorSystem profile.'
}
$null = Assert-PlainPath ([IO.Path]::GetDirectoryName($out)) $true
Assert-Absent $out 'Choose a new evidence directory; this runner does not overwrite an existing directory.'
$null = [IO.Directory]::CreateDirectory($out)
$staticPath = Join-Path $out 'packages.json'
& python -B (Join-Path $PSScriptRoot 'verify-published-release.py') $packages --release-info $metadata --tag $Tag --output $staticPath
if ($LASTEXITCODE -ne 0) { throw 'Package verification did not pass. Do not use these packages for the smoke test.' }
$verified = Get-Content -LiteralPath $staticPath -Raw | ConvertFrom-Json -AsHashtable

$kind = $Format.ToLowerInvariant()
$report = [ordered]@{
  schema = 1
  kind = 'windows-client-launch-smoke-only'
  recorded_at_utc = [datetime]::UtcNow.ToString('o')
  release = $verified.release
  operator_selected_format = $kind
  format_asset = $verified.packages[$kind]
  executable = $verified.portable_binary
  host = [ordered]@{ version = $os.Version; build = $os.BuildNumber; product_type = $os.ProductType; architecture = $arch }
  checks = [ordered]@{
    fresh_profile_and_unelevated_runner = 'Pass'
    desktop_executable_matches_verified_bytes = 'Not run'
    responding_main_window = 'Not run'
    fresh_profile_database_created = 'Not run'
    observed_exit_code_zero = 'Not run'
  }
  status = 'Blocked'
  not_tested = @('Installer identity/registration, install, upgrade, shortcuts, removal and UAC',
    'Rendered WebView2 contents and native IPC', 'Watch, cleanup, recovery, accessibility and mixed DPI')
}
$observed = $null
$phase = 'desktop executable verification'
try {
  $report.status = 'Fail'
  $report.checks.desktop_executable_matches_verified_bytes = 'Fail'
  Assert-Binary $exe $verified.portable_binary
  $report.checks.desktop_executable_matches_verified_bytes = 'Pass'
  $phase = 'fresh profile recheck'
  $report.status = 'Blocked'
  $report.checks.fresh_profile_and_unelevated_runner = 'Blocked'
  Assert-Absent $profile $freshMessage
  $report.checks.fresh_profile_and_unelevated_runner = 'Pass'
  $runner = [Diagnostics.Process]::GetCurrentProcess()
  try { $session = $runner.SessionId } finally { $runner.Dispose() }
  $name = [IO.Path]::GetFileNameWithoutExtension($exe)
  $since = [datetime]::UtcNow
  Write-Host "In Explorer, launch the verified desktop executable now (within $StartupTimeoutSeconds seconds):"
  Write-Host $exe
  Write-Host 'Keep security controls in place; if Windows blocks launch, record that separately.'
  $phase = 'native startup observation'
  $report.status = 'Fail'
  $report.checks.responding_main_window = 'Fail'
  $watch = [Diagnostics.Stopwatch]::StartNew()
  while ($null -eq $observed -and $watch.Elapsed.TotalSeconds -lt $StartupTimeoutSeconds) {
    foreach ($proc in @(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
      try {
        if ($null -eq $observed -and $proc.SessionId -eq $session -and -not $proc.HasExited -and
            $proc.StartTime.ToUniversalTime() -ge $since -and
            $proc.Path -ieq $exe -and $proc.MainWindowHandle -ne [IntPtr]::Zero -and
            $proc.MainWindowTitle -ceq 'ExorSystem' -and $proc.Responding) {
          # Keep a handle to the observed instance, even if it exits before the wait begins.
          $null = $proc.SafeHandle
          $observed = $proc
        }
      } catch [InvalidOperationException], [ComponentModel.Win32Exception] { } # Process can exit during observation.
      finally { if ($proc -ne $observed) { $proc.Dispose() } }
    }
    if ($null -eq $observed) { Start-Sleep -Milliseconds 200 }
  }
  if ($null -eq $observed) { throw 'No responding ExorSystem window from the verified path was observed in this session.' }
  $report.startup_elapsed_ms = $watch.ElapsedMilliseconds
  $phase = 'desktop executable recheck'
  $report.checks.desktop_executable_matches_verified_bytes = 'Fail'
  Assert-Binary $exe $verified.portable_binary
  $report.checks.desktop_executable_matches_verified_bytes = 'Pass'
  $report.checks.responding_main_window = 'Pass'

  $phase = 'fresh profile database observation'
  $report.checks.fresh_profile_database_created = 'Fail'
  $db = Assert-PlainPath (Join-Path $profile 'exor.db') $false
  if ((Get-Item -LiteralPath $db).Length -le 0) { throw 'The new profile database is empty.' }
  $report.checks.fresh_profile_database_created = 'Pass'

  $phase = 'normal exit observation'
  $report.checks.observed_exit_code_zero = 'Fail'
  Write-Host "A responding main window was observed. Close that ExorSystem window normally within $CloseTimeoutSeconds seconds."
  if (-not $observed.WaitForExit($CloseTimeoutSeconds * 1000)) { throw 'The observed desktop did not exit within the close window; the runner did not stop it.' }
  $report.exit_code = $observed.ExitCode
  if ($report.exit_code -ne 0) { throw 'The observed desktop did not exit with code zero.' }
  $report.checks.observed_exit_code_zero = 'Pass'
  $report.status = 'Pass'
} catch {
  # Keep account names/paths and unrestricted error text out of the shareable report.
  $report.stopped_at = $phase
  $report.error_type = $_.Exception.GetType().Name
  Write-Warning "Smoke stopped during ${phase}: $($_.Exception.Message)"
} finally {
  if ($null -ne $observed) { $observed.Dispose() }
  Write-NewJson (Join-Path $out 'launch.json') $report
}

Write-Host "Client launch smoke: $($report.status). See packages.json and launch.json in the evidence directory."
if ($report.status -ne 'Pass') { exit 1 }

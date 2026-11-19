<#
.SYNOPSIS
  Build ExorSystem release artifacts into .\dist
.DESCRIPTION
  Produces ExorSystem-v{VERSION}-x64-Setup.exe, ExorSystem-v{VERSION}-x64.msi,
  ExorSystem-v{VERSION}-x64-Portable.zip (desktop app plus the read-only exor command line)
  and SHA256SUMS.txt.
  Needs: Rust (MSVC toolchain from rust-toolchain.toml), Node 22+, VS Build Tools (C++), WebView2.
  The desktop exe runs asInvoker. Installing or removing machine-wide packages can prompt for UAC.
#>
[CmdletBinding()]
param([string]$OutDir = (Join-Path $PSScriptRoot '..\dist'))
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^\s*version\s*=\s*"([^"]+)"' |
  Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { throw 'Could not read version from Cargo.toml' }
$config = Get-Content -Raw (Join-Path $root 'gui\src-tauri\tauri.conf.json') | ConvertFrom-Json
if ($config.version -ne $version) { throw 'Workspace and Tauri versions differ' }
Write-Host "Building ExorSystem v$version (x86_64-pc-windows-msvc)"

$gui = Join-Path $root 'gui'
Push-Location $gui
try {
  npm ci
  if ($LASTEXITCODE) { throw 'npm ci failed' }
  $buildArgs = @('run', 'tauri', '--', 'build', '--target', 'x86_64-pc-windows-msvc', '--', '--locked')
  & npm @buildArgs
  if ($LASTEXITCODE) { throw 'tauri build failed' }
} finally { Pop-Location }

$metadata = cargo metadata --manifest-path (Join-Path $gui 'src-tauri\Cargo.toml') --no-deps --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'cargo metadata failed' }
$target = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc\release'

# Read-only command line, shipped in the portable ZIP next to the desktop exe.
cargo build --manifest-path (Join-Path $root 'Cargo.toml') --release --locked -p exor-cli --target x86_64-pc-windows-msvc
if ($LASTEXITCODE) { throw 'exor-cli build failed' }
$workspace = cargo metadata --manifest-path (Join-Path $root 'Cargo.toml') --no-deps --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'cargo metadata failed' }
$cli = Get-Item -LiteralPath (Join-Path $workspace.target_directory 'x86_64-pc-windows-msvc\release\exor.exe')
$pkg = "$($config.productName)_${version}_x64"
$exe = Get-Item -LiteralPath (Join-Path $target 'exorsystem-gui.exe')
$nsis = Get-Item -LiteralPath (Join-Path $target "bundle\nsis\$pkg-setup.exe")
$msi = Get-Item -LiteralPath (Join-Path $target "bundle\msi\${pkg}_en-US.msi")

New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = Resolve-Path $OutDir
$prefix = "ExorSystem-v$version-x64"
Copy-Item $nsis.FullName (Join-Path $OutDir "$prefix-Setup.exe") -Force
Copy-Item $msi.FullName (Join-Path $OutDir "$prefix.msi") -Force

# Portable: desktop exe, command line and docs; both use the same per-user profile as the installed builds.
$stage = Join-Path $env:TEMP "exor-portable-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $stage | Out-Null
try {
  Copy-Item $exe.FullName (Join-Path $stage 'ExorSystem.exe')
  Copy-Item $cli.FullName (Join-Path $stage 'exor.exe')
  foreach ($d in 'README.md', 'LICENSE', 'CHANGELOG.md', 'CREDITS.md') {
    $p = Join-Path $root $d
    if (Test-Path $p) { Copy-Item $p $stage }
  }
  $zip = Join-Path $OutDir "$prefix-Portable.zip"
  if (Test-Path $zip) { Remove-Item $zip }
  Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
} finally { Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue }

$sums = @("$prefix-Setup.exe", "$prefix.msi", "$prefix-Portable.zip") | Sort-Object | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash -LiteralPath (Join-Path $OutDir $_) -Algorithm SHA256).Hash.ToLowerInvariant(), $_
}
Set-Content -Path (Join-Path $OutDir 'SHA256SUMS.txt') -Value $sums -Encoding ascii
Write-Host "Done. Artifacts in $OutDir"
$sums | ForEach-Object { Write-Host $_ }

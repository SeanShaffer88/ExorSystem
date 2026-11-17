<#
.SYNOPSIS
  Build ExorSystem release artifacts into .\dist
.DESCRIPTION
  Produces ExorSystem-v{VERSION}-x64-Setup.exe, ExorSystem-v{VERSION}-x64.msi,
  ExorSystem-v{VERSION}-x64-Portable.zip and SHA256SUMS.txt.
  Needs: Rust (MSVC toolchain from rust-toolchain.toml), Node 22+, VS Build Tools (C++), WebView2.
  The app embeds a requireAdministrator manifest, so the built exe prompts for UAC.
#>
[CmdletBinding()]
param([string]$OutDir = (Join-Path $PSScriptRoot '..\dist'))
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^\s*version\s*=\s*"([^"]+)"' |
  Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { throw 'Could not read version from Cargo.toml' }
Write-Host "Building ExorSystem v$version"

$gui = Join-Path $root 'gui'
Push-Location $gui
try {
  npm ci
  if ($LASTEXITCODE) { throw 'npm ci failed' }
  npm run tauri -- build
  if ($LASTEXITCODE) { throw 'tauri build failed' }
} finally { Pop-Location }

$target = Join-Path $gui 'src-tauri\target\release'
$exe = Get-ChildItem $target -Filter '*.exe' -File | Where-Object { $_.Name -notmatch 'build|setup' } | Select-Object -First 1
$nsis = Get-ChildItem (Join-Path $target 'bundle\nsis') -Filter '*-setup.exe' | Select-Object -First 1
$msi = Get-ChildItem (Join-Path $target 'bundle\msi') -Filter '*.msi' | Select-Object -First 1
foreach ($f in @($exe, $nsis, $msi)) { if (-not $f) { throw 'A build output is missing; see the Tauri build log above.' } }

New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = Resolve-Path $OutDir
$prefix = "ExorSystem-v$version-x64"
Copy-Item $nsis.FullName (Join-Path $OutDir "$prefix-Setup.exe") -Force
Copy-Item $msi.FullName (Join-Path $OutDir "$prefix.msi") -Force

# Portable: exe + docs; the app keeps its data in .\data next to the exe.
$stage = Join-Path $env:TEMP "exor-portable-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $stage | Out-Null
try {
  Copy-Item $exe.FullName (Join-Path $stage 'ExorSystem.exe')
  foreach ($d in 'README.md', 'LICENSE', 'CHANGELOG.md', 'CREDITS.md') {
    $p = Join-Path $root $d
    if (Test-Path $p) { Copy-Item $p $stage }
  }
  $zip = Join-Path $OutDir "$prefix-Portable.zip"
  if (Test-Path $zip) { Remove-Item $zip }
  Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
} finally { Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue }

$sums = Get-ChildItem $OutDir -Filter "$prefix*" -File | Sort-Object Name | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
Set-Content -Path (Join-Path $OutDir 'SHA256SUMS.txt') -Value $sums -Encoding ascii
Write-Host "Done. Artifacts in $OutDir"
$sums | ForEach-Object { Write-Host $_ }

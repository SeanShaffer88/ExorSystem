#requires -RunAsAdministrator
<#
 Development-only loader for exordrv.sys: enables test signing, creates and trusts a
 self-signed certificate, signs the driver, registers the minifilter and loads it.
 Use on a disposable VM. Never on a machine you care about.
 Usage:  .\dev-load.ps1 -Sys .\exordrv.sys        (load)
         .\dev-load.ps1 -Unload                   (unload and remove)
#>
param(
    [string]$Sys = (Join-Path $PSScriptRoot '..\x64\Release\exordrv.sys'),
    [switch]$Unload
)
$ErrorActionPreference = 'Stop'
$svc  = 'ExorDrv'
$inst = 'ExorDrv Instance'
$dest = Join-Path $env:SystemRoot 'System32\drivers\exordrv.sys'
$key  = "HKLM:\SYSTEM\CurrentControlSet\Services\$svc"

if ($Unload) {
    fltmc unload $svc 2>$null
    sc.exe delete $svc | Out-Null
    Remove-Item $dest -Force -ErrorAction SilentlyContinue
    Write-Host 'ExorDrv unloaded and removed.'
    return
}

if (-not (Test-Path $Sys)) { throw "Driver not found: $Sys (build it first, see driver/README.md)" }

# 1. Test signing (needs Secure Boot off; takes effect after reboot)
if (-not (bcdedit /enum '{current}' | Select-String -Pattern 'testsigning\s+Yes')) {
    bcdedit /set testsigning on | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'bcdedit failed. If it says the value is protected by Secure Boot policy, disable Secure Boot in firmware (VM only).' }
    Write-Host 'Test signing enabled. REBOOT, then run this script again.'
    exit 2
}

# 2. Self-signed code-signing certificate, trusted machine-wide
$subject = 'CN=ExorSystem Test Signing'
$cert = Get-ChildItem Cert:\LocalMachine\My | Where-Object Subject -eq $subject | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject -CertStoreLocation Cert:\LocalMachine\My -NotAfter (Get-Date).AddYears(2)
    foreach ($name in 'Root', 'TrustedPublisher') {
        $store = New-Object Security.Cryptography.X509Certificates.X509Store($name, 'LocalMachine')
        $store.Open('ReadWrite'); $store.Add($cert); $store.Close()
    }
}

# 3. Sign (signtool ships with the Windows SDK); no timestamp server, stays offline
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw 'signtool.exe not found. Install the Windows SDK.' }
& $signtool.FullName sign /fd SHA256 /sha1 $cert.Thumbprint $Sys
if ($LASTEXITCODE -ne 0) { throw 'signtool failed' }

# 4. Register minifilter service + instance, then load
Copy-Item $Sys $dest -Force
if (-not (Test-Path $key)) {
    sc.exe create $svc type= filesys binPath= $dest start= demand group= 'FSFilter Activity Monitor' depend= FltMgr | Out-Null
}
reg.exe add "HKLM\SYSTEM\CurrentControlSet\Services\$svc\Instances" /v DefaultInstance /t REG_SZ /d $inst /f | Out-Null
reg.exe add "HKLM\SYSTEM\CurrentControlSet\Services\$svc\Instances\$inst" /v Altitude /t REG_SZ /d 385201 /f | Out-Null
reg.exe add "HKLM\SYSTEM\CurrentControlSet\Services\$svc\Instances\$inst" /v Flags /t REG_DWORD /d 0 /f | Out-Null
fltmc load $svc
if ($LASTEXITCODE -ne 0) { throw 'fltmc load failed. See the troubleshooting table in driver/README.md.' }
Write-Host 'ExorDrv loaded. Verify with: fltmc filters'

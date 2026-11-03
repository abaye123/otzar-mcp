# Signs a file with the owner's code-signing certificate ("abaye ai", CurrentUser\My).
# Usage: powershell -File scripts\sign.ps1 target\release\otzar-mcp.exe
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = 'Stop'
$signtool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
$thumbprint = 'EBAE118B2C6434273890DE749EC457C34FE1A882'

if (-not (Test-Path $signtool)) { throw "signtool not found at $signtool" }
if (-not (Test-Path $File)) { throw "file not found: $File" }

& $signtool sign /sha1 $thumbprint /fd sha256 /tr http://timestamp.digicert.com /td sha256 $File
if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }

# The certificate is self-signed, so Status is UnknownError; what matters is the signer and the timestamp.
$sig = Get-AuthenticodeSignature $File
if ($sig.SignerCertificate.Thumbprint -ne $thumbprint) { throw 'unexpected signer thumbprint' }
if (-not $sig.TimeStamperCertificate) { throw 'signature has no timestamp' }
Write-Host "Signed $File (signer $($sig.SignerCertificate.Subject), timestamped)"

<#
.SYNOPSIS
  Creates a SELF-SIGNED code-signing certificate for testing the release
  pipeline's Windows signing path.

.DESCRIPTION
  The certificate is NOT trusted by Windows or SmartScreen; installers signed
  with it still show "Unknown publisher". Use it only to verify that the
  pipeline imports the certificate and signs correctly. Because the root is
  untrusted, Get-AuthenticodeSignature reports NotTrusted rather than Valid,
  so the release workflow's "Valid" check is expected to fail with this
  certificate unless the runner trusts it. Use a real certificate for releases.

  Outputs (in -OutDir, default ./test-cert) are secrets: do not commit them.
    test-cert.pfx      the certificate
    test-cert.pfx.b64  base64 text for the WINDOWS_CERTIFICATE secret
    password.txt       value for the WINDOWS_CERTIFICATE_PASSWORD secret

.EXAMPLE
  ./scripts/new-test-certificate.ps1 -Subject "CN=OpenDeck test"
#>
param(
  [string]$Subject = "CN=OpenDeck test signing",
  [string]$OutDir = "test-cert"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Subject `
  -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
  -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(1)

$bytes = New-Object byte[] 18
[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$plain = [Convert]::ToBase64String($bytes)
$password = ConvertTo-SecureString -String $plain -AsPlainText -Force

$pfx = Join-Path $OutDir "test-cert.pfx"
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $password | Out-Null
[Convert]::ToBase64String([IO.File]::ReadAllBytes($pfx)) |
  Set-Content -NoNewline (Join-Path $OutDir "test-cert.pfx.b64")
Set-Content -NoNewline -Path (Join-Path $OutDir "password.txt") -Value $plain

Write-Host "Thumbprint: $($cert.Thumbprint)"
Write-Host "Wrote $OutDir\test-cert.pfx.b64 and $OutDir\password.txt"
Write-Host "Add them as secrets WINDOWS_CERTIFICATE and WINDOWS_CERTIFICATE_PASSWORD."
Write-Host "WARNING: self-signed, NOT trusted by SmartScreen. Do not commit $OutDir."

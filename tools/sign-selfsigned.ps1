#!/usr/bin/env pwsh
<#
  sign-selfsigned.ps1 — helper per firma Authenticode con certificato SELF-SIGNED.

  SCOPO: testare in locale la pipeline di firma (UAC/SmartScreen) senza comprare
  un certificato. NON serve per la distribuzione pubblica: un cert self-signed è
  fidato solo sulle macchine dove lo installi (vedi CODE-SIGNING.md).

  Esempi:
    # crea il cert "Borderline RP" e rendilo fidato su QUESTA macchina (utente corrente)
    pwsh -File tools/sign-selfsigned.ps1 -CreateCert -Trust

    # firma un file con un thumbprint già esistente
    pwsh -File tools/sign-selfsigned.ps1 -Thumbprint ABC123... -File "src-tauri\target\release\Borderline RP Launcher.exe"

    # crea + firma in un colpo
    pwsh -File tools/sign-selfsigned.ps1 -CreateCert -Trust -File "src-tauri\target\release\Borderline RP Launcher.exe"

    # rimuovi i cert self-signed "Borderline RP" creati da questo script
    pwsh -File tools/sign-selfsigned.ps1 -Remove

  NOTE:
   - Usa gli store CurrentUser (niente admin). Per un test UAC "di sistema"
     completo servirebbe LocalMachine\Root (richiede admin): vedi -MachineTrust.
   - La firma include un timestamp, così resta valida oltre la scadenza del cert.
#>
[CmdletBinding(DefaultParameterSetName = 'Sign')]
param(
  [switch]$CreateCert,
  [switch]$Trust,         # importa il cert in CurrentUser Root + TrustedPublisher
  [switch]$MachineTrust,  # come -Trust ma in LocalMachine (richiede admin)
  [string]$Thumbprint,
  [string]$File,
  [switch]$Remove,
  [string]$Subject   = 'CN=Borderline RP, O=Borderline RP, C=IT',
  [string]$Friendly  = 'Borderline RP (self-signed)',
  [string]$TimestampUrl = 'http://timestamp.digicert.com'
)

$ErrorActionPreference = 'Stop'
function Info($m){ Write-Host $m -ForegroundColor Cyan }
function Ok($m){ Write-Host "  v $m" -ForegroundColor Green }
function Warn($m){ Write-Host "  ! $m" -ForegroundColor Yellow }

# --- Remove --------------------------------------------------------------------
if ($Remove) {
  $found = Get-ChildItem Cert:\CurrentUser\My, Cert:\CurrentUser\Root, Cert:\CurrentUser\TrustedPublisher -ErrorAction SilentlyContinue |
    Where-Object { $_.Subject -like '*Borderline RP*' -and $_.Issuer -eq $_.Subject }  # self-signed: issuer == subject
  if (-not $found) { Warn "Nessun cert self-signed 'Borderline RP' trovato."; return }
  foreach ($c in $found) {
    Info "Rimuovo $($c.Thumbprint) da $($c.PSParentPath -replace '.*::','')"
    Remove-Item $c.PSPath -Force
  }
  Ok "Rimozione completata."
  return
}

# --- CreateCert ----------------------------------------------------------------
$cert = $null
if ($CreateCert) {
  Info "Creo certificato di code-signing self-signed..."
  $cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject $Subject `
    -FriendlyName $Friendly `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -KeyUsage DigitalSignature `
    -KeyExportPolicy Exportable `
    -HashAlgorithm SHA256 `
    -NotAfter (Get-Date).AddYears(3)
  Ok "Creato. Thumbprint: $($cert.Thumbprint)"
  $Thumbprint = $cert.Thumbprint
}

# Risolvi il cert dal thumbprint se non l'abbiamo appena creato
if (-not $cert) {
  if (-not $Thumbprint) {
    # prova a trovarne uno già presente
    $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -like '*Borderline RP*' } | Select-Object -First 1
    if ($cert) { $Thumbprint = $cert.Thumbprint; Info "Uso cert esistente: $Thumbprint" }
  } else {
    $cert = Get-Item "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction SilentlyContinue
  }
}

# --- Trust ---------------------------------------------------------------------
function Import-To($cert, $storePath) {
  $store = $storePath -replace '.*\\',''
  $loc   = if ($storePath -like 'Cert:\LocalMachine\*') { 'LocalMachine' } else { 'CurrentUser' }
  # esporta solo la parte pubblica e importa nello store di fiducia
  $tmp = Join-Path $env:TEMP "brp-selfsigned.cer"
  Export-Certificate -Cert $cert -FilePath $tmp -Force | Out-Null
  Import-Certificate -FilePath $tmp -CertStoreLocation $storePath | Out-Null
  Remove-Item $tmp -Force -ErrorAction SilentlyContinue
  Ok "Importato in $loc\$store"
}

if ($Trust -or $MachineTrust) {
  if (-not $cert) { throw "Nessun certificato da rendere fidato (usa -CreateCert o -Thumbprint)." }
  if ($MachineTrust) {
    Warn "Trust a livello di MACCHINA (serve un terminale come amministratore)."
    Import-To $cert 'Cert:\LocalMachine\Root'
    Import-To $cert 'Cert:\LocalMachine\TrustedPublisher'
  } else {
    Import-To $cert 'Cert:\CurrentUser\Root'
    Import-To $cert 'Cert:\CurrentUser\TrustedPublisher'
  }
}

# --- Sign ----------------------------------------------------------------------
if ($File) {
  if (-not $cert) { throw "Nessun certificato disponibile per firmare (usa -CreateCert o -Thumbprint)." }
  if (-not (Test-Path -LiteralPath $File)) { throw "File non trovato: $File" }
  Info "Firmo: $File"
  $res = Set-AuthenticodeSignature -FilePath $File -Certificate $cert `
            -TimestampServer $TimestampUrl -HashAlgorithm SHA256
  if ($res.Status -ne 'Valid') {
    Warn "Stato firma: $($res.Status) — $($res.StatusMessage)"
    Warn "Se non è 'Valid', spesso è perché il cert non è in Trusted Root/Publishers: rilancia con -Trust."
  } else {
    Ok "Firma valida. Publisher: $($cert.Subject)"
  }
}

if (-not ($CreateCert -or $Trust -or $MachineTrust -or $File -or $Remove)) {
  Info "Niente da fare. Esempi:"
  Info "  pwsh -File tools/sign-selfsigned.ps1 -CreateCert -Trust"
  Info "  pwsh -File tools/sign-selfsigned.ps1 -Thumbprint <T> -File `"...\\Borderline RP Launcher.exe`""
}

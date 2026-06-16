#requires -version 5
<#
  Installa-Certificato-BorderlineRP.ps1

  Da eseguire UNA VOLTA prima di installare il Launcher di Borderline RP.
  Rende il PC capace di riconoscere "Borderline RP" come editore fidato, così
  l'installer e il launcher non vengono segnalati come "editore sconosciuto".

  COSA FA (in modo trasparente):
   - importa il certificato pubblico Borderline-RP.cer (che si trova in questa
     stessa cartella) negli store "Autorità di certificazione radice attendibili"
     e "Editori attendibili" del COMPUTER.
   - non installa programmi, non raccoglie dati, non si connette a internet.

  USO:
   - tasto destro sul file  ->  "Esegui con PowerShell"
     (oppure):  pwsh -ExecutionPolicy Bypass -File .\Installa-Certificato-BorderlineRP.ps1
   - accetta il prompt UAC (serve per scrivere negli store del computer).
#>

$ErrorActionPreference = 'Stop'

# --- Trova il .cer accanto a questo script ------------------------------------
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$cer  = Join-Path $here 'Borderline-RP.cer'
if (-not (Test-Path $cer)) {
  Write-Host "ERRORE: 'Borderline-RP.cer' non trovato accanto a questo script." -ForegroundColor Red
  Read-Host "Premi Invio per chiudere"; exit 1
}

# --- Auto-elevazione: scrivere negli store del COMPUTER richiede admin ---------
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
  Write-Host "Richiedo i permessi di amministratore..." -ForegroundColor Yellow
  $psExe = (Get-Process -Id $PID).Path   # powershell.exe o pwsh.exe
  Start-Process $psExe -Verb RunAs -ArgumentList @(
    '-ExecutionPolicy','Bypass','-File',"`"$($MyInvocation.MyCommand.Path)`""
  )
  exit
}

# --- Mostra all'utente cosa sta per fidarsi -----------------------------------
$c = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 $cer
Write-Host ""
Write-Host "  Certificato:  $($c.Subject)"   -ForegroundColor Cyan
Write-Host "  Impronta:     $($c.Thumbprint)" -ForegroundColor Cyan
Write-Host "  Valido fino:  $($c.NotAfter.ToString('dd/MM/yyyy'))" -ForegroundColor Cyan
Write-Host ""

# --- Importa nei due store del computer ---------------------------------------
function Import-To([string]$storeName) {
  $store = New-Object System.Security.Cryptography.X509Certificates.X509Store(
    $storeName, [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
  $store.Open('ReadWrite')
  $store.Add($c)
  $store.Close()
  Write-Host "  v Installato in: LocalMachine\$storeName" -ForegroundColor Green
}
Import-To 'Root'             # Autorità di certificazione radice attendibili
Import-To 'TrustedPublisher' # Editori attendibili

Write-Host ""
Write-Host "  Fatto! Ora puoi installare il Launcher di Borderline RP senza" -ForegroundColor Green
Write-Host "  l'avviso di 'editore sconosciuto'." -ForegroundColor Green
Write-Host ""
Read-Host "Premi Invio per chiudere"

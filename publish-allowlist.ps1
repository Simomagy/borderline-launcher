#!/usr/bin/env pwsh
<#
  publish-allowlist.ps1 — firma e pubblica l'allowlist anti-dump sul CDN.

  NIENTE ricompilazione, NIENTE release: aggiorni `anticheat/allowlist.json`,
  lanci questo script, e in ~30s tutti i launcher aperti hanno la lista nuova
  (il thread anti-cheat fa una GET condizionale ad ogni scan). Vedi
  ALLOWLIST-CDN-SPEC.md per il quadro completo.

  Flusso: bump automatico di `version` -> valida JSON -> firma minisign
          (stessa chiave delle release) -> PUT byte-verbatim al CDN.

  Credenziali (lette da .release.env, fallback .env — come release.ps1):
    - CDN_API_KEY                          (Bearer per il PUT)
    - TAURI_SIGNING_PRIVATE_KEY            (path o contenuto della chiave minisign)
    - TAURI_SIGNING_PRIVATE_KEY_PASSWORD   (vuota se la chiave non ha passphrase,
                                            ma comunque DEFINITA per evitare prompt)

  Uso:
    ./publish-allowlist.ps1            # bump + firma + pubblica
    ./publish-allowlist.ps1 -NoBump    # pubblica la version corrente (re-publish)
    ./publish-allowlist.ps1 -DryRun    # bump + firma in locale, NIENTE upload
    ./publish-allowlist.ps1 -Commit    # committa allowlist.json dopo l'upload
#>
param(
  [switch]$NoBump,
  [switch]$DryRun,
  [switch]$Commit
)

$ErrorActionPreference = 'Stop'

if ($PSVersionTable.PSVersion.Major -lt 7) {
  Write-Host "`n  x Serve PowerShell 7 (pwsh)." -ForegroundColor Red
  Write-Host "    pwsh -File `"$PSCommandPath`"" -ForegroundColor Yellow
  Read-Host "`nPremi Invio per chiudere"; exit 1
}

$PSNativeCommandUseErrorActionPreference = $false
$root = $PSScriptRoot

trap {
  Write-Host "`n  x ERRORE: $($_.Exception.Message)" -ForegroundColor Red
  if ($_.ScriptStackTrace) { Write-Host "    $($_.ScriptStackTrace -replace "`n","`n    ")" -ForegroundColor DarkGray }
  Read-Host "`nPremi Invio per chiudere"; exit 1
}

function Fail($m) { throw $m }
function Ok($m)   { Write-Host "  v $m" -ForegroundColor Green }
function Info($m) { Write-Host $m       -ForegroundColor Cyan }

# Endpoint CDN (vedi ALLOWLIST-CDN-SPEC.md §4.A). Memorizza i byte VERBATIM.
$PutUrl   = 'https://cdn.borderlinerp.com/api/anticheat-allowlist'
$JsonPath = Join-Path $root 'anticheat/allowlist.json'
$SigPath  = "$JsonPath.sig"
if (-not (Test-Path -LiteralPath $JsonPath)) { Fail "Non trovo $JsonPath" }

# --- 1. Carica credenziali (.release.env ha precedenza, poi .env) ------------
function Load-EnvFile([string]$path, [switch]$Overwrite) {
  if (-not (Test-Path $path)) { return }
  Get-Content $path | ForEach-Object {
    if ($_ -match '^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*)$') {
      $k = $Matches[1]; $v = $Matches[2].Trim().Trim('"').Trim("'")
      if ($Overwrite -or $null -eq [Environment]::GetEnvironmentVariable($k)) {
        [Environment]::SetEnvironmentVariable($k, $v)
      }
    }
  }
}
Load-EnvFile (Join-Path $root '.release.env') -Overwrite
Load-EnvFile (Join-Path $root '.env')

if (-not [Environment]::GetEnvironmentVariable('CDN_API_KEY'))             { Fail "CDN_API_KEY mancante (.release.env o .env)" }
$signKey = [Environment]::GetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY')
if (-not $signKey) { Fail "TAURI_SIGNING_PRIVATE_KEY mancante" }
# `tauri signer sign` vuole il CONTENUTO della chiave (a differenza di `tauri
# build`, che risolve anche un path). Se il valore e' un path a un file
# esistente, leggine il contenuto — come fa release.ps1 nel runner.
if (Test-Path -LiteralPath $signKey -PathType Leaf) {
  [Environment]::SetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', (Get-Content -LiteralPath $signKey -Raw).Trim())
}
# La password puo' essere vuota, ma DEVE essere definita o tauri signer prompta.
if ($null -eq [Environment]::GetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY_PASSWORD')) {
  [Environment]::SetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY_PASSWORD', '')
}

# --- 2. Bump version (regex sul testo grezzo: NON tocca la formattazione/array) ---
$raw = Get-Content -LiteralPath $JsonPath -Raw
if ($raw -notmatch '"version"\s*:\s*(\d+)') { Fail 'Campo "version" (intero) non trovato in allowlist.json' }
$cur = [int]$Matches[1]
$ver = if ($NoBump) { $cur } else { $cur + 1 }
if (-not $NoBump) {
  $raw = [regex]::Replace($raw, '("version"\s*:\s*)\d+', "`${1}$ver")
  Set-Content -LiteralPath $JsonPath -Value $raw -Encoding utf8NoBOM -NoNewline
}
Ok "version: $cur -> $ver$(if($NoBump){' (NoBump)'})"

# --- 3. Valida il JSON (parse di controllo, senza riscrivere) ----------------
try {
  $doc = Get-Content -LiteralPath $JsonPath -Raw | ConvertFrom-Json -ErrorAction Stop
} catch { Fail "allowlist.json non e' JSON valido: $($_.Exception.Message)" }
$nOwner  = @($doc.owner_allow).Count
$nWindow = @($doc.window_class_block).Count
Ok "owner_allow: $nOwner nomi | window_class_block: $nWindow"

# --- 4. Firma minisign (stessa keypair delle release) ------------------------
# tauri signer legge TAURI_SIGNING_PRIVATE_KEY(_PASSWORD) dall'ambiente e
# scrive <file>.sig (formato Tauri = base64 della firma minisign, una riga).
if (Test-Path -LiteralPath $SigPath) { Remove-Item -LiteralPath $SigPath -Force }
Info "`nFirma minisign..."
Push-Location $root
try { bunx tauri signer sign "$JsonPath" } finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { Fail "tauri signer sign fallito (exit $LASTEXITCODE)" }
if (-not (Test-Path -LiteralPath $SigPath)) { Fail "Firma non prodotta: $SigPath" }
$sig = (Get-Content -LiteralPath $SigPath -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($sig)) { Fail "Firma vuota in $SigPath" }
if ($sig -match "`n") { Fail "La firma contiene newline: non puo' viaggiare in un header HTTP" }
Ok "Firma: $($sig.Length) char"

if ($DryRun) {
  Info "`n[DryRun] STOP — niente upload. JSON firmato in locale (version $ver)."
  Read-Host "`nPremi Invio"; exit 0
}

# --- 5. PUT byte-verbatim al CDN --------------------------------------------
# curl --data-binary @file invia i byte ESATTI del file (gli stessi firmati):
# il CDN li memorizza verbatim e li serve identici -> la firma resta valida.
$curl = Join-Path $env:WINDIR 'System32\curl.exe'
if (-not (Test-Path $curl)) { Fail "curl.exe non trovato in System32" }

Info "PUT allowlist al CDN..."
$resp = & $curl --silent --show-error --fail-with-body `
  -X PUT `
  -H "Authorization: Bearer $env:CDN_API_KEY" `
  -H "Content-Type: application/json" `
  -H "X-Minisig: $sig" `
  --data-binary "@$JsonPath" `
  $PutUrl 2>&1
if ($LASTEXITCODE -ne 0) { Fail "PUT fallito (curl exit $LASTEXITCODE). Risposta: $resp" }
Ok "Pubblicata version $ver"
Info "  -> https://cdn.borderlinerp.com/anticheat/latest.json"
if ($resp) { Info "  risposta CDN: $resp" }

# --- 6. Commit opzionale (solo allowlist.json; niente tag/CI/build) ----------
if ($Commit) {
  Info "`nCommit di allowlist.json..."
  git -C $root add -- 'anticheat/allowlist.json'
  git -C $root commit -m "anticheat: allowlist v$ver" | Out-Null
  if ($LASTEXITCODE -eq 0) { Ok "Committato" } else { Info "  (niente da committare)" }
}

Info "`nFatto. version $ver live (~30s di propagazione ai launcher aperti)."
Read-Host "`nPremi Invio per chiudere"

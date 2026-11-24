<#
.SYNOPSIS
  Compila, assina e publica uma versão do SavesGG no repositório público de lançamentos.

.DESCRIPTION
  1. Confere se está na main com a árvore limpa (exceto em -Ensaio).
  2. Lê a chave privada de %USERPROFILE%\.tauri\savesgg.key (nunca impressa).
  3. Compila com `npm run tauri build` (gera o instalador e a assinatura .sig).
  4. Copia o instalador com um nome estável (SavesGG_x64-setup.exe) e monta o zip portable.
  5. Gera latest.json (lido pela atualização automática) e SHA256SUMS.txt.
  6. Publica a release vX.Y.Z em thelipesousa/savesgg-releases (exceto em -Ensaio).

.EXAMPLE
  powershell -File scripts/lancar.ps1 -Ensaio
  powershell -File scripts/lancar.ps1 -Notas "Restaurar vários jogos de uma vez, log e atualização automática."
#>
param(
  [switch]$Ensaio,
  [string]$Notas = ""
)

$ErrorActionPreference = "Stop"
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz

$repoLancamentos = "thelipesousa/savesgg-releases"
$chave = Join-Path $env:USERPROFILE ".tauri\savesgg.key"

# 1. Estado do Git
if (-not $Ensaio) {
  $branch = (git rev-parse --abbrev-ref HEAD).Trim()
  if ($branch -ne "main") { throw "Lance a partir da main (agora em '$branch')." }
  if (git status --porcelain) { throw "Há mudanças não commitadas." }
}

# 2. Chave de assinatura
if (-not (Test-Path $chave)) { throw "Chave não encontrada em $chave. Sem ela não há como assinar." }
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $chave -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""

# Ids do cadastro do login na nuvem (fora do Git). Sem eles, o login fica desligado.
$oauth = Join-Path $env:USERPROFILE ".tauri\savesgg-oauth.json"
if (Test-Path $oauth) {
  $o = Get-Content $oauth -Raw | ConvertFrom-Json
  $env:SAVESGG_GOOGLE_CLIENT_ID = "$($o.google_client_id)"
  $env:SAVESGG_GOOGLE_CLIENT_SECRET = "$($o.google_client_secret)"
  Write-Host "Login no Google Drive: ligado" -ForegroundColor Green
} else {
  Write-Warning "Login na nuvem ficará desligado nesta versão (falta $oauth)."
}

$versao = (Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
$tag = "v$versao"
Write-Host "SavesGG $versao" -ForegroundColor Cyan

# 3. Compilação
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "A compilação falhou." }

$nsis = "target\release\bundle\nsis"
$setup = Join-Path $nsis "SavesGG_${versao}_x64-setup.exe"
$sig = "$setup.sig"
if (-not (Test-Path $setup)) { throw "Instalador não encontrado: $setup" }
if (-not (Test-Path $sig)) { throw "Assinatura não encontrada: $sig (createUpdaterArtifacts ligado?)" }

# 4. Nome estável
$saida = "target\release\lancamento"
New-Item -ItemType Directory -Force $saida | Out-Null
Copy-Item $setup (Join-Path $saida "SavesGG_x64-setup.exe") -Force
Copy-Item $sig (Join-Path $saida "SavesGG_x64-setup.exe.sig") -Force

# 4b. Versão portable (pendrive): exe + manifesto + pasta "dados", que liga o modo portable.
$port = Join-Path $saida "portable"
if (Test-Path $port) { Remove-Item -Recurse -Force $port }
New-Item -ItemType Directory -Force (Join-Path $port "resources"), (Join-Path $port "dados") | Out-Null
Copy-Item "targetelease\savesgg.exe" (Join-Path $port "SavesGG.exe")
Copy-Item "src-tauriesources\manifest.yaml" (Join-Path $port "resources\manifest.yaml")
$leia = @"
SavesGG portable

Esta pasta guarda as configuracoes, o historico e os logs do SavesGG quando ele roda
do pendrive. Nao apague: sem ela o SavesGG usa as pastas do Windows, como o instalado.

- Nao inicia com o Windows: abra o SavesGG.exe quando plugar o pendrive.
- Atualizacao: o app avisa e abre a pagina de downloads; troque o SavesGG.exe pelo novo.
- Login do Google: fica guardado em cada PC (entre de novo em cada um).
"@
[System.IO.File]::WriteAllText((Resolve-Path (Join-Path $port "dados")).Path + "\LEIA-ME.txt", $leia, (New-Object System.Text.UTF8Encoding $false))
$zip = Join-Path $saida "SavesGG_portable_x64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $port "*") -DestinationPath $zip
Remove-Item -Recurse -Force $port

# 5. latest.json e SHA256SUMS.txt
if (-not $Notas) {
  $Notas = (git tag -l --format="%(contents)" $tag | Out-String).Trim()
  if (-not $Notas) { $Notas = "SavesGG $versao" }
}
$latest = [ordered]@{
  version   = $versao
  notes     = $Notas
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = (Get-Content $sig -Raw).Trim()
      url       = "https://github.com/$repoLancamentos/releases/download/$tag/SavesGG_x64-setup.exe"
    }
  }
}
$latestPath = Join-Path $saida "latest.json"
[System.IO.File]::WriteAllText((Resolve-Path $saida).Path + "\latest.json", ($latest | ConvertTo-Json -Depth 5), (New-Object System.Text.UTF8Encoding $false))

$somas = Get-ChildItem $saida -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
[System.IO.File]::WriteAllLines((Resolve-Path $saida).Path + "\SHA256SUMS.txt", $somas, (New-Object System.Text.UTF8Encoding $false))

Write-Host "Arquivos em $saida" -ForegroundColor Green
Get-ChildItem $saida | Format-Table Name, Length

# 6. Publicação
if ($Ensaio) {
  Write-Host "Ensaio: nada foi publicado." -ForegroundColor Yellow
  exit 0
}
$arquivos = Get-ChildItem $saida -File | ForEach-Object { $_.FullName }
gh release create $tag --repo $repoLancamentos --title "SavesGG $versao" --notes $Notas @arquivos
if ($LASTEXITCODE -ne 0) { throw "A publicação falhou." }
Write-Host "Publicado: https://github.com/$repoLancamentos/releases/tag/$tag" -ForegroundColor Green

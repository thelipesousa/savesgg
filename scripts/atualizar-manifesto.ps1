# Baixa o manifesto de saves do Ludusavi (MIT, dados da PCGamingWiki) para
# src-tauri/resources/manifest.yaml, que vai embutido no instalador como reserva offline.
$ErrorActionPreference = 'Stop'
$url = 'https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml'
$destino = Join-Path $PSScriptRoot '..\src-tauri\resources\manifest.yaml'
New-Item -ItemType Directory -Force (Split-Path $destino) | Out-Null
Invoke-WebRequest -Uri $url -OutFile $destino -UseBasicParsing
$tamanho = [math]::Round((Get-Item $destino).Length / 1MB, 1)
Write-Host "Manifesto atualizado: $destino ($tamanho MB)"

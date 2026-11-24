# Versão portable (modo pendrive) — design curto

Issue: #33.

## Intenção
Levar o SavesGG num pendrive e usar em vários PCs sem instalar. Tudo do app (configuração, fila, histórico, logs) anda junto no pendrive.

## Abordagens
1. **Pasta `dados` ao lado do exe liga o modo portable** (escolhida). Um só binário; o zip traz a pasta. Sem flag de compilação, sem instalador diferente.
2. Binário separado compilado com feature `portable`: dobra o build e o release, sem ganho para o usuário.
3. Argumento `--portable` na linha de comando: frágil (atalho errado = dados espalhados no PC).

## Desenho
- `savesgg_core::pastas::Pastas { config, local, portatil }`, decidida uma vez (`Pastas::atual()`):
  - existe `<pasta do exe>\dados` → `config = local = dados`, `portatil = true`;
  - senão → `%APPDATA%\SavesGG` e `%LOCALAPPDATA%\SavesGG` (como hoje).
- Quem hoje monta esses caminhos passa a usar `Pastas::atual()`: `Config::default_path`, fila de pendentes, preparação da nuvem, destinos de nuvem.
- Logs: no portable vão para `dados\logs` (e "Abrir pasta de logs" abre ela).
- Migração do nome antigo (SaveGuard) não roda no portable.
- "Iniciar com o Windows": escondido no portable e o app não mexe no registro (a letra do pendrive muda, e o registro de mesmo nome pode ser do SavesGG instalado no PC).
- Atualização: no portable o aviso mostra "Baixar a versão nova", que abre a página de lançamentos (URL fixa no Rust, nada vindo da tela).
- Login do Google: segue no Gerenciador de Credenciais de cada PC (perder o pendrive não vaza a conta).
- Lançamento: `scripts/lancar.ps1` gera `SavesGG_portable_x64.zip` com `SavesGG.exe`, `resources\manifest.yaml` e `dados\LEIA-ME.txt`, e publica junto (entra no SHA256SUMS).

## Fora do escopo
- Cache do WebView2 (`%LOCALAPPDATA%\com.savesgg.app`) continua no PC: é só cache da tela, sem dados do usuário.
- WebView2 precisa existir no PC (já vem no Windows 10/11).

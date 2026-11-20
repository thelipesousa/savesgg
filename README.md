# SaveGuard (nome provisório)

App para Windows que encontra os saves de todos os seus jogos (Steam, Epic, GOG, Xbox, emuladores e jogos sem loja), mostra quais correm risco por não terem cópia fora do PC e faz backup contínuo para nuvem (pasta do OneDrive/Google Drive/Dropbox), pendrive ou HD externo, com versões e restauração num PC novo.

Feito com **Tauri 2 + Rust** (motor) e **Svelte + TypeScript + Tailwind** (interface).

- Design aprovado: [`docs/superpowers/specs/2026-11-20-saveguard-design.md`](docs/superpowers/specs/2026-11-20-saveguard-design.md)
- Referências visuais (Google Stitch): [`design/stitch/`](design/stitch/)

## Como rodar

Requisitos: Windows 10/11, [Rust](https://rustup.rs) estável, Node 22 e o WebView2 (já vem no Windows).

```powershell
npm install
./scripts/atualizar-manifesto.ps1   # baixa o manifesto de saves embutido no app
npm run tauri dev                   # abre o app em modo de desenvolvimento
```

Testes: `cargo test --workspace` (motor e comandos) e `npm run test -- --run` (interface).
Testes com os dados reais do seu PC (não alteram os saves): veja os comandos `--ignored` nos arquivos
`crates/core/src/scan.rs` e `crates/core/tests/ponta_a_ponta_real.rs`.

## Fluxo de branches

- `main`: produção e branch padrão; só código estável, é dela que sai o release.
- `dev`: desenvolvimento e integração; recebe as funcionalidades prontas.
- `feature/<nome>`: uma funcionalidade por branch, criada a partir da `dev`.
- `fix/<nome>`: correção de bug, a partir da `dev`.
- `hotfix/<nome>`: correção urgente em produção, a partir da `main`; depois volta pra `main` e pra `dev`.

Commits no padrão [Conventional Commits](https://www.conventionalcommits.org/pt-br/) (`feat`, `fix`, `docs`…), em português. PRs vão para a `dev`; lançamento é PR `dev` → `main`.

## Créditos

Locais de saves baseados no [ludusavi-manifest](https://github.com/mtkennerly/ludusavi-manifest) (dados da PCGamingWiki).

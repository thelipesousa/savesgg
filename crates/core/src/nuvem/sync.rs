//! Enviador: sincroniza a pasta local de preparação de uma conta com a nuvem.
//!
//! Uma passada: envia o que mudou (o `.zip` antes do `.json` de cada versão e o
//! índice por último), apaga o `.zip` local quando a versão inteira está na nuvem,
//! e apaga na nuvem as versões que a retenção apagou aqui. Só apaga o que está no
//! estado (o que o app enviou ou baixou), nunca outros arquivos.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::nomes::{arquivo_remoto_valido, jogo_valido};
use super::{ErroNuvem, Provedor};
use crate::backup::backup_root;

/// Estado da sincronização, guardado na preparação (fora da pasta `SavesGG`).
pub const ARQUIVO_ESTADO: &str = "estado.json";
const PASTA: &str = "SavesGG";
pub(crate) const INDICE: &str = "SavesGG/biblioteca.json";

/// O que está na nuvem, pelo último envio, download ou listagem.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EstadoNuvem {
    pub arquivos: BTreeMap<String, ArquivoRemoto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArquivoRemoto {
    /// blake3 do conteúdo; vazio quando só foi visto na listagem (zip ainda não baixado).
    pub blake3: String,
    pub tamanho: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumoSync {
    pub enviados: usize,
    pub apagados: usize,
    pub pendentes: usize,
}

pub fn carregar_estado(prep: &Path) -> EstadoNuvem {
    fs::read_to_string(prep.join(ARQUIVO_ESTADO))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn salvar_estado(prep: &Path, e: &EstadoNuvem) -> Result<(), ErroNuvem> {
    let json = serde_json::to_vec_pretty(e).map_err(|x| ErroNuvem::Local(x.to_string()))?;
    fs::create_dir_all(prep).map_err(|x| ErroNuvem::Local(x.to_string()))?;
    crate::backup_write_atomic(&prep.join(ARQUIVO_ESTADO), &json)
        .map_err(|x| ErroNuvem::Local(x.to_string()))
}

pub(crate) fn hash_arquivo(p: &Path) -> Result<(String, u64), ErroNuvem> {
    let dados = fs::read(p).map_err(|e| ErroNuvem::Local(e.to_string()))?;
    Ok((
        blake3::hash(&dados).to_hex().to_string(),
        dados.len() as u64,
    ))
}

/// Uma versão local: jogo, nome, `.json` e o `.zip` (se ainda estiver aqui).
pub(crate) struct VersaoLocal {
    pub jogo: String,
    pub stem: String,
    pub json: PathBuf,
    pub zip: Option<PathBuf>,
}

pub(crate) fn versoes_locais(prep: &Path) -> Vec<VersaoLocal> {
    let mut out = Vec::new();
    let Ok(jogos) = fs::read_dir(backup_root(prep)) else {
        return out;
    };
    for j in jogos.flatten().filter(|e| e.path().is_dir()) {
        let jogo = j.file_name().to_string_lossy().into_owned();
        if !jogo_valido(&jogo) {
            continue;
        }
        let Ok(arqs) = fs::read_dir(j.path()) else {
            continue;
        };
        for a in arqs.flatten() {
            let nome = a.file_name().to_string_lossy().into_owned();
            if let Some((stem, "json")) = arquivo_remoto_valido(&nome) {
                let zip = a.path().with_extension("zip");
                out.push(VersaoLocal {
                    jogo: jogo.clone(),
                    stem,
                    json: a.path(),
                    zip: zip.is_file().then_some(zip),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.jogo, &a.stem).cmp(&(&b.jogo, &b.stem)));
    out
}

pub(crate) fn remoto(jogo: &str, stem: &str, ext: &str) -> String {
    format!("{PASTA}/{jogo}/{stem}.{ext}")
}

/// Envia `local` para `caminho` se o conteúdo for diferente do que está no estado.
fn enviar_se_mudou(
    prov: &dyn Provedor,
    prep: &Path,
    estado: &mut EstadoNuvem,
    caminho: &str,
    local: &Path,
) -> Result<bool, ErroNuvem> {
    let (hash, tamanho) = hash_arquivo(local)?;
    if estado
        .arquivos
        .get(caminho)
        .is_some_and(|a| a.blake3 == hash)
    {
        return Ok(false);
    }
    prov.enviar(caminho, local)?;
    estado.arquivos.insert(
        caminho.to_string(),
        ArquivoRemoto {
            blake3: hash,
            tamanho,
        },
    );
    salvar_estado(prep, estado)?;
    Ok(true)
}

/// Uma passada de sincronização. Chamar com a trava do destino.
pub fn sincronizar(prep: &Path, prov: &dyn Provedor) -> Result<ResumoSync, ErroNuvem> {
    let mut estado = carregar_estado(prep);
    let mut resumo = ResumoSync::default();
    let locais = versoes_locais(prep);

    for v in &locais {
        let cz = remoto(&v.jogo, &v.stem, "zip");
        let cj = remoto(&v.jogo, &v.stem, "json");
        if let Some(zip) = &v.zip {
            resumo.enviados += usize::from(enviar_se_mudou(prov, prep, &mut estado, &cz, zip)?);
        }
        resumo.enviados += usize::from(enviar_se_mudou(prov, prep, &mut estado, &cj, &v.json)?);
        // Versão inteira na nuvem: o zip local pode sair (o índice e o .json ficam).
        if let Some(zip) = &v.zip {
            let (hz, _) = hash_arquivo(zip)?;
            if estado.arquivos.get(&cz).is_some_and(|a| a.blake3 == hz) {
                fs::remove_file(zip).map_err(|e| ErroNuvem::Local(e.to_string()))?;
            }
        }
    }

    let indice = prep.join(INDICE);
    if indice.is_file() {
        resumo.enviados += usize::from(enviar_se_mudou(prov, prep, &mut estado, INDICE, &indice)?);
    }

    // Retenção: versões que estão no estado mas cujo .json local sumiu.
    let vivas: BTreeSet<(String, String)> = locais
        .iter()
        .map(|v| (v.jogo.clone(), v.stem.clone()))
        .collect();
    let apagar: Vec<String> = estado
        .arquivos
        .keys()
        .filter(|k| versao_do_caminho(k).is_some_and(|v| !vivas.contains(&v)))
        .cloned()
        .collect();
    for c in apagar {
        prov.apagar(&c)?;
        estado.arquivos.remove(&c);
        salvar_estado(prep, &estado)?;
        resumo.apagados += 1;
    }

    resumo.pendentes = pendentes(prep);
    Ok(resumo)
}

/// Baixa para a preparação os `.json` das versões que estão na nuvem e ainda não
/// estão aqui (de outro PC ou de antes de formatar), e registra no estado tudo o que
/// viu. Nomes fora do padrão são ignorados. Devolve quantos `.json` baixou.
pub fn atualizar_da_nuvem(prep: &Path, prov: &dyn Provedor) -> Result<usize, ErroNuvem> {
    let mut estado = carregar_estado(prep);
    let mut baixados = 0;
    for pasta in prov.listar(PASTA)? {
        if !pasta.pasta || !jogo_valido(&pasta.nome) {
            continue;
        }
        let jogo = pasta.nome;
        for item in prov.listar(&format!("{PASTA}/{jogo}"))? {
            if item.pasta {
                continue;
            }
            let Some((stem, ext)) = arquivo_remoto_valido(&item.nome) else {
                continue;
            };
            let caminho = remoto(&jogo, &stem, ext);
            let local = backup_root(prep).join(&jogo).join(format!("{stem}.{ext}"));
            if ext == "json" {
                if !local.exists() {
                    baixar_atomico(prov, &caminho, &local)?;
                    baixados += 1;
                    let (h, t) = hash_arquivo(&local)?;
                    estado.arquivos.insert(
                        caminho,
                        ArquivoRemoto {
                            blake3: h,
                            tamanho: t,
                        },
                    );
                }
            } else {
                estado.arquivos.entry(caminho).or_insert(ArquivoRemoto {
                    blake3: String::new(),
                    tamanho: item.tamanho,
                });
            }
        }
    }
    salvar_estado(prep, &estado)?;
    if baixados > 0 {
        incluir_no_indice(prep)?;
    }
    Ok(baixados)
}

/// Põe no índice local as versões cujo `.json` está aqui mas o `.zip` só na nuvem
/// (o índice comum só junta versões com o `.zip` ao lado).
fn incluir_no_indice(prep: &Path) -> Result<(), ErroNuvem> {
    let erro = |e: crate::CoreError| ErroNuvem::Local(e.to_string());
    let mut idx = crate::backup::LibraryIndex::load(prep).map_err(erro)?;
    for v in versoes_locais(prep) {
        let refs = idx.games.entry(v.jogo.clone()).or_default();
        if refs.iter().any(|r| r.stem == v.stem) {
            continue;
        }
        let Ok(meta) = crate::backup::load_meta(prep, &v.jogo, &v.stem) else {
            continue;
        };
        refs.push(crate::backup::SnapshotRef {
            stem: v.stem,
            created: meta.created,
            pinned: meta.pinned,
            content_hash: meta.content_hash,
            total_bytes: meta.total_bytes,
            seguranca: meta.seguranca,
        });
        refs.sort_by_key(|r| r.created);
    }
    idx.save_atomic(prep).map_err(erro)
}

/// Baixa o `.zip` de uma versão para a preparação, se ainda não estiver aqui
/// (antes de restaurar de um destino de nuvem).
pub fn garantir_zip(
    prep: &Path,
    prov: &dyn Provedor,
    jogo: &str,
    stem: &str,
) -> Result<(), ErroNuvem> {
    if !jogo_valido(jogo) || !super::nomes::versao_valida(stem) {
        return Err(ErroNuvem::Local("nome de versão inválido".into()));
    }
    let local = backup_root(prep).join(jogo).join(format!("{stem}.zip"));
    if local.is_file() {
        return Ok(());
    }
    let caminho = remoto(jogo, stem, "zip");
    baixar_atomico(prov, &caminho, &local)?;
    // Registra o conteúdo real, para o enviador não mandar de volta.
    let (h, t) = hash_arquivo(&local)?;
    let mut estado = carregar_estado(prep);
    estado.arquivos.insert(
        caminho,
        ArquivoRemoto {
            blake3: h,
            tamanho: t,
        },
    );
    salvar_estado(prep, &estado)
}

/// Baixa para `<destino>.tmp` e renomeia: nunca deixa arquivo pela metade.
fn baixar_atomico(prov: &dyn Provedor, caminho: &str, destino: &Path) -> Result<(), ErroNuvem> {
    let local_err = |e: std::io::Error| ErroNuvem::Local(e.to_string());
    if let Some(d) = destino.parent() {
        fs::create_dir_all(d).map_err(local_err)?;
    }
    let mut tmp = destino.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if let Err(e) = prov.baixar(caminho, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, destino).map_err(local_err)
}

/// Data da versão mais recente de cada jogo que já está inteira na nuvem (zip e json
/// no estado). Versão que ainda não subiu não protege o jogo.
pub fn ultimas_enviadas(prep: &Path) -> BTreeMap<String, chrono::DateTime<chrono::Utc>> {
    let estado = carregar_estado(prep);
    let mut out: BTreeMap<String, chrono::DateTime<chrono::Utc>> = BTreeMap::new();
    for v in versoes_locais(prep) {
        let na_nuvem = estado
            .arquivos
            .contains_key(&remoto(&v.jogo, &v.stem, "zip"))
            && estado
                .arquivos
                .contains_key(&remoto(&v.jogo, &v.stem, "json"));
        if !na_nuvem {
            continue;
        }
        let Ok(meta) = crate::backup::load_meta(prep, &v.jogo, &v.stem) else {
            continue;
        };
        out.entry(v.jogo)
            .and_modify(|d| *d = (*d).max(meta.created))
            .or_insert(meta.created);
    }
    out
}

/// `SavesGG/<jogo>/<stem>.<ext>` → (jogo, stem), só para nomes válidos.
pub(crate) fn versao_do_caminho(c: &str) -> Option<(String, String)> {
    let resto = c.strip_prefix("SavesGG/")?;
    let (jogo, arq) = resto.split_once('/')?;
    if !jogo_valido(jogo) {
        return None;
    }
    let (stem, _) = arquivo_remoto_valido(arq)?;
    Some((jogo.to_string(), stem))
}

/// Arquivos locais que ainda não estão na nuvem do jeito que estão aqui.
pub fn pendentes(prep: &Path) -> usize {
    let estado = carregar_estado(prep);
    let difere = |caminho: &str, local: &Path| {
        hash_arquivo(local)
            .map(|(h, _)| estado.arquivos.get(caminho).is_none_or(|a| a.blake3 != h))
            .unwrap_or(false)
    };
    let mut n = 0;
    for v in versoes_locais(prep) {
        if let Some(zip) = &v.zip {
            n += usize::from(difere(&remoto(&v.jogo, &v.stem, "zip"), zip));
        }
        n += usize::from(difere(&remoto(&v.jogo, &v.stem, "json"), &v.json));
    }
    let indice = prep.join(INDICE);
    if indice.is_file() {
        n += usize::from(difere(INDICE, &indice));
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{BackupOutcome, LibraryIndex, backup_game, set_pinned};
    use crate::manifest::{CloudFlags, Store};
    use crate::nuvem::falso::ProvedorFalso;
    use crate::paths::PathContext;
    use crate::scan::{DetectedGame, Source};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use std::fs;
    use std::path::Path;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 21, 40, 5).unwrap()
    }

    fn ctx(root: &Path) -> PathContext {
        let home = root.join("Users/Ana");
        PathContext {
            app_data: home.join("AppData/Roaming"),
            local_app_data: home.join("AppData/Local"),
            local_app_data_low: home.join("AppData/LocalLow"),
            documents: home.join("Documents"),
            public: root.join("Users/Public"),
            program_data: root.join("ProgramData"),
            os_user_name: "Ana".into(),
            store_user_id: None,
            home,
        }
    }

    /// Faz um backup do jogo "hades" com `conteudo`, `seg` segundos depois de `now()`.
    fn backup(root: &Path, prep: &Path, conteudo: &str, seg: i64, keep: usize) -> String {
        let c = ctx(root);
        let save = c.app_data.join("Hades");
        fs::create_dir_all(&save).unwrap();
        fs::write(save.join("slot.sav"), conteudo).unwrap();
        let g = DetectedGame {
            id: "hades".into(),
            name: "Hades".into(),
            store: Store::SemLoja,
            save_roots: vec![save.clone()],
            files: vec![save.join("slot.sav")],
            total_bytes: conteudo.len() as u64,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
            steam_app_id: None,
            last_modified: None,
        };
        fs::create_dir_all(prep).unwrap();
        match backup_game(&g, &c, prep, keep, now() + Duration::seconds(seg)).unwrap() {
            BackupOutcome::Criado(r) => r.stem,
            BackupOutcome::SemMudanca => panic!("sem mudança"),
        }
    }

    fn zip_local(prep: &Path, stem: &str) -> std::path::PathBuf {
        prep.join(format!("SavesGG/hades/{stem}.zip"))
    }

    #[test]
    fn sync_envia_versao_e_libera_zip() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let stem = backup(t.path(), &prep, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        let r = sincronizar(&prep, &nuvem).unwrap();
        assert_eq!(r.enviados, 3);
        assert!(nuvem.tem(&format!("SavesGG/hades/{stem}.zip")));
        assert!(nuvem.tem(&format!("SavesGG/hades/{stem}.json")));
        assert!(nuvem.tem("SavesGG/biblioteca.json"));
        assert!(!zip_local(&prep, &stem).exists(), "zip local liberado");
        assert!(prep.join(format!("SavesGG/hades/{stem}.json")).is_file());
        assert_eq!(pendentes(&prep), 0);
    }

    #[test]
    fn sync_sem_mudanca_nao_reenvia() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        backup(t.path(), &prep, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        sincronizar(&prep, &nuvem).unwrap();
        assert_eq!(sincronizar(&prep, &nuvem).unwrap().enviados, 0);
    }

    #[test]
    fn sync_versao_fixada_reenvia_json() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let stem = backup(t.path(), &prep, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        sincronizar(&prep, &nuvem).unwrap();
        set_pinned(&prep, "hades", &stem, true).unwrap();
        let antes = nuvem.envios();
        let r = sincronizar(&prep, &nuvem).unwrap();
        // o .json da versão e o índice (que também guarda o "fixada")
        assert_eq!(r.enviados, 2);
        assert_eq!(nuvem.envios() - antes, 2);
        let json = String::from_utf8(nuvem.ler(&format!("SavesGG/hades/{stem}.json"))).unwrap();
        assert!(json.contains("\"pinned\": true"));
    }

    #[test]
    fn sync_retencao_apaga_na_nuvem() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let velha = backup(t.path(), &prep, "v1", 0, 1);
        let nuvem = ProvedorFalso::default();
        sincronizar(&prep, &nuvem).unwrap();
        let nova = backup(t.path(), &prep, "v2", 60, 1);
        let r = sincronizar(&prep, &nuvem).unwrap();
        assert_eq!(r.apagados, 2);
        assert!(!nuvem.tem(&format!("SavesGG/hades/{velha}.zip")));
        assert!(!nuvem.tem(&format!("SavesGG/hades/{velha}.json")));
        assert!(nuvem.tem(&format!("SavesGG/hades/{nova}.zip")));
        assert_eq!(LibraryIndex::load(&prep).unwrap().games["hades"].len(), 1);
    }

    #[test]
    fn sync_nao_apaga_o_que_nao_esta_no_estado() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        backup(t.path(), &prep, "v1", 0, 1);
        let nuvem = ProvedorFalso::default();
        nuvem.por("SavesGG/outro/2026-01-01_00-00-00.zip", b"de outro lugar");
        sincronizar(&prep, &nuvem).unwrap();
        backup(t.path(), &prep, "v2", 60, 1);
        sincronizar(&prep, &nuvem).unwrap();
        sincronizar(&prep, &nuvem).unwrap();
        assert!(nuvem.tem("SavesGG/outro/2026-01-01_00-00-00.zip"));
    }

    #[test]
    fn sync_falha_no_meio_retoma() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let stem = backup(t.path(), &prep, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        nuvem.falhar_apos(1);
        assert!(matches!(
            sincronizar(&prep, &nuvem),
            Err(ErroNuvem::Rede(_))
        ));
        assert!(
            zip_local(&prep, &stem).exists(),
            "nada liberado com a versão incompleta"
        );
        assert!(pendentes(&prep) > 0);
        nuvem.sem_falha();
        sincronizar(&prep, &nuvem).unwrap();
        assert!(nuvem.tem(&format!("SavesGG/hades/{stem}.json")));
        assert!(nuvem.tem("SavesGG/biblioteca.json"));
        assert!(!zip_local(&prep, &stem).exists());
        assert_eq!(pendentes(&prep), 0);
    }

    #[test]
    fn pull_traz_versoes_de_outro_pc() {
        let t = tempfile::tempdir().unwrap();
        let outro = t.path().join("outro-pc");
        let stem = backup(t.path(), &outro, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        sincronizar(&outro, &nuvem).unwrap();
        let prep = t.path().join("pc-novo");
        assert_eq!(atualizar_da_nuvem(&prep, &nuvem).unwrap(), 1);
        let idx = LibraryIndex::load(&prep).unwrap();
        assert_eq!(idx.games["hades"][0].stem, stem);
        assert!(
            prep.join("SavesGG/biblioteca.json").is_file(),
            "destino passa a ter backup"
        );
        assert_eq!(atualizar_da_nuvem(&prep, &nuvem).unwrap(), 0);
        // nada a reenviar além do índice deste PC
        assert_eq!(sincronizar(&prep, &nuvem).unwrap().enviados, 1);
    }

    #[test]
    fn pull_ignora_nomes_invalidos() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let nuvem = ProvedorFalso::default();
        nuvem.por("SavesGG/../x.json", b"{}");
        nuvem.por("SavesGG/Jogo X/2026-11-20_21-40-05.json", b"{}");
        nuvem.por("SavesGG/jogo/a.exe", b"MZ");
        nuvem.por("SavesGG/jogo/../../fora.json", b"{}");
        assert_eq!(atualizar_da_nuvem(&prep, &nuvem).unwrap(), 0);
        assert!(!t.path().join("x.json").exists());
        assert!(!t.path().join("fora.json").exists());
        assert!(!prep.join("SavesGG/jogo/a.exe").exists());
        assert!(!prep.join("SavesGG/Jogo X").exists());
    }

    #[test]
    fn garantir_zip_baixa_so_quando_falta() {
        let t = tempfile::tempdir().unwrap();
        let outro = t.path().join("outro-pc");
        let stem = backup(t.path(), &outro, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        sincronizar(&outro, &nuvem).unwrap();
        let prep = t.path().join("pc-novo");
        atualizar_da_nuvem(&prep, &nuvem).unwrap();
        let antes = nuvem.baixados();
        garantir_zip(&prep, &nuvem, "hades", &stem).unwrap();
        assert!(zip_local(&prep, &stem).is_file());
        assert_eq!(nuvem.baixados(), antes + 1);
        garantir_zip(&prep, &nuvem, "hades", &stem).unwrap();
        assert_eq!(nuvem.baixados(), antes + 1);
        // o zip baixado não volta a subir
        assert_eq!(sincronizar(&prep, &nuvem).unwrap().enviados, 1); // só o índice
        assert!(garantir_zip(&prep, &nuvem, "../x", &stem).is_err());
    }

    #[test]
    fn ultimas_enviadas_ignora_nao_enviadas() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        backup(t.path(), &prep, "v1", 0, 10);
        assert!(ultimas_enviadas(&prep).is_empty());
        let nuvem = ProvedorFalso::default();
        sincronizar(&prep, &nuvem).unwrap();
        assert_eq!(ultimas_enviadas(&prep)["hades"], now());
    }

    #[test]
    fn sync_erro_de_login_nao_apaga_nada() {
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let stem = backup(t.path(), &prep, "v1", 0, 10);
        let nuvem = ProvedorFalso::default();
        nuvem.sempre_login();
        assert!(matches!(sincronizar(&prep, &nuvem), Err(ErroNuvem::Login)));
        assert!(zip_local(&prep, &stem).exists());
        assert!(!prep.join(ARQUIVO_ESTADO).exists() || pendentes(&prep) > 0);
    }
}

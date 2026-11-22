//! Provedor Google Drive (API v3, escopo `drive.file`).
//!
//! A raiz é a pasta "SavesGG Backups" na raiz do Drive. O app só enxerga o que ele
//! criou, por isso resolve cada pasta pelo nome e guarda os ids em `ids.json` (caminho
//! → id). Um id que sumiu (404) limpa o cache e a pasta é procurada de novo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;

use super::http::{Http, Pedido, Resposta, Token, codificar};
use super::{ErroNuvem, ItemRemoto, Provedor};

pub const API: &str = "https://www.googleapis.com/drive/v3/files";
pub const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
pub const PASTA_RAIZ: &str = "SavesGG Backups";
const TIPO_PASTA: &str = "application/vnd.google-apps.folder";
/// Até aqui vai num envio só; acima, upload "resumable" em pedaços.
const LIMITE_SIMPLES: usize = 5 * 1024 * 1024;
/// Pedaço do upload "resumable" (múltiplo de 256 KiB, como a API exige). Cada pedaço
/// tem o próprio prazo, então um zip grande em internet lenta termina de subir.
pub const PEDACO: usize = 8 * 1024 * 1024;
/// Marca interna de "não achou" (404), para refazer com o cache limpo.
const NAO_ACHOU: &str = "HTTP 404";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lista {
    #[serde(default)]
    files: Vec<ArquivoDrive>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArquivoDrive {
    id: String,
    name: String,
    mime_type: Option<String>,
    size: Option<String>,
}

#[derive(Deserialize)]
struct Criado {
    id: String,
}

/// Escapa um valor para ir entre aspas simples na `q` da API.
pub fn escapar_q(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

pub struct GoogleDrive<H: Http, T: Token> {
    http: H,
    token: T,
    arquivo_ids: PathBuf,
    ids: Mutex<BTreeMap<String, String>>,
    /// A pasta raiz do cache já foi conferida nesta instância (uma vez por passada).
    raiz_conferida: Mutex<bool>,
}

impl<H: Http, T: Token> GoogleDrive<H, T> {
    /// `arquivo_ids`: o `ids.json` dentro da pasta de preparação da conta.
    pub fn new(http: H, token: T, arquivo_ids: PathBuf) -> Self {
        let ids = std::fs::read_to_string(&arquivo_ids)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        GoogleDrive {
            http,
            token,
            arquivo_ids,
            ids: Mutex::new(ids),
            raiz_conferida: Mutex::new(false),
        }
    }

    fn salvar_ids(&self, ids: &BTreeMap<String, String>) {
        if let Ok(json) = serde_json::to_string(ids) {
            if let Some(d) = self.arquivo_ids.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = crate::backup_write_atomic(&self.arquivo_ids, json.as_bytes());
        }
    }

    fn limpar_ids(&self) {
        let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
        ids.clear();
        self.salvar_ids(&ids);
    }

    /// Faz a requisição com o token; num 401 renova e tenta de novo uma vez.
    fn chamar(&self, p: Pedido) -> Result<Resposta, ErroNuvem> {
        let com_token = |t: String| {
            let mut q = p.clone();
            q.cabecalhos
                .push(("Authorization".into(), format!("Bearer {t}")));
            q
        };
        let r = self.http.enviar(com_token(self.token.atual()?))?;
        if r.status != 401 {
            return Ok(r);
        }
        let r = self.http.enviar(com_token(self.token.renovar()?))?;
        if r.status == 401 {
            return Err(ErroNuvem::Login);
        }
        Ok(r)
    }

    fn checar(r: Resposta) -> Result<Resposta, ErroNuvem> {
        match r.status {
            s if (200..300).contains(&s) => Ok(r),
            404 => Err(ErroNuvem::Servidor(NAO_ACHOU.into())),
            // A pessoa desmarcou a permissão do Drive no login: só entrando de novo.
            403 if {
                let corpo = String::from_utf8_lossy(&r.corpo);
                corpo.contains("insufficientPermissions")
                    || corpo.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT")
            } =>
            {
                Err(ErroNuvem::Login)
            }
            s => Err(ErroNuvem::Servidor(format!("HTTP {s}"))),
        }
    }

    fn json<D: for<'a> Deserialize<'a>>(r: &Resposta) -> Result<D, ErroNuvem> {
        serde_json::from_slice(&r.corpo)
            .map_err(|e| ErroNuvem::Servidor(format!("resposta inesperada do Drive: {e}")))
    }

    /// Lista os itens que casam com `q` (todas as páginas).
    fn consultar(&self, q: &str) -> Result<Vec<ArquivoDrive>, ErroNuvem> {
        let mut out = Vec::new();
        let mut pagina: Option<String> = None;
        loop {
            let mut url = format!(
                "{API}?q={}&fields={}&pageSize=1000&spaces=drive",
                codificar(q),
                codificar("nextPageToken,files(id,name,mimeType,size)")
            );
            if let Some(p) = &pagina {
                url.push_str(&format!("&pageToken={}", codificar(p)));
            }
            let r = Self::checar(self.chamar(Pedido::new("GET", url))?)?;
            let lista: Lista = Self::json(&r)?;
            out.extend(lista.files);
            match lista.next_page_token {
                Some(p) => pagina = Some(p),
                None => return Ok(out),
            }
        }
    }

    fn criar_pasta(&self, nome: &str, pai: &str) -> Result<String, ErroNuvem> {
        let meta = serde_json::json!({ "name": nome, "mimeType": TIPO_PASTA, "parents": [pai] });
        let r = Self::checar(
            self.chamar(
                Pedido::new("POST", format!("{API}?fields=id"))
                    .cabecalho("Content-Type", "application/json; charset=UTF-8")
                    .corpo(meta.to_string().into_bytes()),
            )?,
        )?;
        Ok(Self::json::<Criado>(&r)?.id)
    }

    /// Id da pasta `caminho` (relativo à raiz do app). Com `criar`, cria o que faltar;
    /// sem, devolve `None` se alguma parte não existir.
    fn pasta_id(&self, caminho: &str, criar: bool) -> Result<Option<String>, ErroNuvem> {
        let mut chave = String::new();
        let mut pai = "root".to_string();
        let segmentos =
            std::iter::once(PASTA_RAIZ).chain(caminho.split('/').filter(|s| !s.is_empty()));
        for (i, seg) in segmentos.enumerate() {
            let k = if i == 0 {
                "/".to_string()
            } else {
                if !chave.is_empty() {
                    chave.push('/');
                }
                chave.push_str(seg);
                chave.clone()
            };
            if i == 0 {
                self.conferir_raiz()?;
            }
            let cache = self
                .ids
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&k)
                .cloned();
            let id = match cache {
                Some(id) => id,
                None => {
                    let q = format!(
                        "name='{}' and '{}' in parents and mimeType='{TIPO_PASTA}' and trashed=false",
                        escapar_q(seg),
                        escapar_q(&pai)
                    );
                    let achado = self.consultar(&q)?.into_iter().next().map(|a| a.id);
                    let id = match achado {
                        Some(id) => id,
                        None if criar => self.criar_pasta(seg, &pai)?,
                        None => return Ok(None),
                    };
                    let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
                    ids.insert(k, id.clone());
                    self.salvar_ids(&ids);
                    id
                }
            };
            pai = id;
        }
        Ok(Some(pai))
    }

    /// Uma vez por instância: se a pasta raiz guardada no cache foi para a lixeira ou
    /// sumiu, esquece o cache (senão os backups iriam para a lixeira e se perderiam).
    fn conferir_raiz(&self) -> Result<(), ErroNuvem> {
        let mut conferida = self
            .raiz_conferida
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *conferida {
            return Ok(());
        }
        let raiz = self
            .ids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("/")
            .cloned();
        if let Some(id) = raiz {
            #[derive(Deserialize)]
            struct Lixeira {
                #[serde(default)]
                trashed: bool,
            }
            let r = self.chamar(Pedido::new("GET", format!("{API}/{id}?fields=trashed")))?;
            let perdida = match r.status {
                404 => true,
                s if (200..300).contains(&s) => Self::json::<Lixeira>(&r)?.trashed,
                _ => Self::checar(r).map(|_| false)?,
            };
            if perdida {
                self.limpar_ids();
            }
        }
        *conferida = true;
        Ok(())
    }

    fn arquivo_id(&self, pasta: &str, nome: &str) -> Result<Option<String>, ErroNuvem> {
        let q = format!(
            "name='{}' and '{}' in parents and trashed=false",
            escapar_q(nome),
            escapar_q(pasta)
        );
        Ok(self
            .consultar(&q)?
            .into_iter()
            .find(|a| a.mime_type.as_deref() != Some(TIPO_PASTA))
            .map(|a| a.id))
    }

    /// Roda a operação; se um id do cache sumiu (404), limpa o cache e tenta de novo.
    fn com_cache<R>(&self, op: impl Fn() -> Result<R, ErroNuvem>) -> Result<R, ErroNuvem> {
        match op() {
            Err(ErroNuvem::Servidor(m)) if m == NAO_ACHOU => {
                self.limpar_ids();
                op()
            }
            r => r,
        }
    }
}

impl<H: Http, T: Token> GoogleDrive<H, T> {
    /// Manda o arquivo na sessão "resumable", um pedaço por vez (308 = continue).
    fn enviar_em_pedacos(
        &self,
        sessao: &str,
        origem: &Path,
        tamanho: usize,
    ) -> Result<(), ErroNuvem> {
        use std::io::Read;
        let mut f = std::fs::File::open(origem).map_err(|e| ErroNuvem::Local(e.to_string()))?;
        let mut inicio = 0usize;
        loop {
            let n = PEDACO.min(tamanho - inicio);
            let mut buf = vec![0u8; n];
            f.read_exact(&mut buf)
                .map_err(|e| ErroNuvem::Local(e.to_string()))?;
            let r = self.chamar(
                Pedido::new("PUT", sessao)
                    .cabecalho(
                        "Content-Range",
                        format!("bytes {inicio}-{}/{tamanho}", inicio + n - 1),
                    )
                    .corpo(buf),
            )?;
            inicio += n;
            if r.status == 308 && inicio < tamanho {
                continue;
            }
            Self::checar(r)?;
            if inicio < tamanho {
                return Err(ErroNuvem::Servidor(
                    "o Drive encerrou o envio antes do fim".into(),
                ));
            }
            return Ok(());
        }
    }
}

fn separar(caminho: &str) -> (&str, &str) {
    caminho.rsplit_once('/').unwrap_or(("", caminho))
}

impl<H: Http, T: Token> Provedor for GoogleDrive<H, T> {
    fn listar(&self, pasta: &str) -> Result<Vec<ItemRemoto>, ErroNuvem> {
        self.com_cache(|| {
            let Some(id) = self.pasta_id(pasta, false)? else {
                return Ok(Vec::new());
            };
            let q = format!("'{}' in parents and trashed=false", escapar_q(&id));
            Ok(self
                .consultar(&q)?
                .into_iter()
                .map(|a| ItemRemoto {
                    pasta: a.mime_type.as_deref() == Some(TIPO_PASTA),
                    tamanho: a.size.and_then(|s| s.parse().ok()).unwrap_or(0),
                    nome: a.name,
                })
                .collect())
        })
    }

    fn enviar(&self, caminho: &str, origem: &Path) -> Result<(), ErroNuvem> {
        let tamanho = std::fs::metadata(origem)
            .map_err(|e| ErroNuvem::Local(e.to_string()))?
            .len() as usize;
        let (dir, nome) = separar(caminho);
        self.com_cache(|| {
            let pai = self
                .pasta_id(dir, true)?
                .ok_or_else(|| ErroNuvem::Servidor("pasta não criada".into()))?;
            let existente = self.arquivo_id(&pai, nome)?;
            if tamanho <= LIMITE_SIMPLES {
                let dados = std::fs::read(origem).map_err(|e| ErroNuvem::Local(e.to_string()))?;
                let pedido = match &existente {
                    Some(id) => Pedido::new("PATCH", format!("{UPLOAD}/{id}?uploadType=media"))
                        .cabecalho("Content-Type", "application/octet-stream")
                        .corpo(dados),
                    None => {
                        let meta = serde_json::json!({ "name": nome, "parents": [pai] });
                        let fronteira = "savesgg_fronteira_7f3a";
                        let mut corpo = format!(
                            "--{fronteira}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{fronteira}\r\nContent-Type: application/octet-stream\r\n\r\n"
                        )
                        .into_bytes();
                        corpo.extend_from_slice(&dados);
                        corpo.extend_from_slice(format!("\r\n--{fronteira}--").as_bytes());
                        Pedido::new("POST", format!("{UPLOAD}?uploadType=multipart&fields=id"))
                            .cabecalho(
                                "Content-Type",
                                format!("multipart/related; boundary={fronteira}"),
                            )
                            .corpo(corpo)
                    }
                };
                Self::checar(self.chamar(pedido)?)?;
                return Ok(());
            }
            // Arquivo grande: abre uma sessão "resumable" e manda o conteúdo nela.
            let inicio = match &existente {
                Some(id) => Pedido::new("PATCH", format!("{UPLOAD}/{id}?uploadType=resumable")),
                None => Pedido::new("POST", format!("{UPLOAD}?uploadType=resumable"))
                    .corpo(serde_json::json!({ "name": nome, "parents": [pai] }).to_string().into_bytes()),
            }
            .cabecalho("Content-Type", "application/json; charset=UTF-8")
            .cabecalho("X-Upload-Content-Length", tamanho.to_string());
            let r = Self::checar(self.chamar(inicio)?)?;
            let sessao = r
                .cabecalho("Location")
                .ok_or_else(|| ErroNuvem::Servidor("o Drive não abriu a sessão de envio".into()))?
                .to_string();
            self.enviar_em_pedacos(&sessao, origem, tamanho)
        })
    }

    fn baixar(&self, caminho: &str, destino: &Path) -> Result<(), ErroNuvem> {
        let (dir, nome) = separar(caminho);
        let r = self.com_cache(|| {
            let pai = self
                .pasta_id(dir, false)?
                .ok_or_else(|| ErroNuvem::Servidor(format!("não existe na nuvem: {caminho}")))?;
            let id = self
                .arquivo_id(&pai, nome)?
                .ok_or_else(|| ErroNuvem::Servidor(format!("não existe na nuvem: {caminho}")))?;
            Self::checar(self.chamar(Pedido::new("GET", format!("{API}/{id}?alt=media")))?)
        })?;
        std::fs::write(destino, &r.corpo).map_err(|e| ErroNuvem::Local(e.to_string()))
    }

    fn apagar(&self, caminho: &str) -> Result<(), ErroNuvem> {
        let (dir, nome) = separar(caminho);
        let Some(pai) = self.pasta_id(dir, false)? else {
            return Ok(());
        };
        let Some(id) = self.arquivo_id(&pai, nome)? else {
            return Ok(());
        };
        let r = self.chamar(Pedido::new("DELETE", format!("{API}/{id}")))?;
        if r.ok() || r.status == 404 {
            Ok(())
        } else {
            Err(ErroNuvem::Servidor(format!("HTTP {}", r.status)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nuvem::http::{Pedido, Resposta};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    #[derive(Default)]
    struct HttpGravado {
        respostas: Mutex<VecDeque<Resposta>>,
        pedidos: Mutex<Vec<Pedido>>,
    }

    impl HttpGravado {
        fn com(respostas: Vec<(u16, &str)>) -> Self {
            let h = HttpGravado::default();
            for (s, c) in respostas {
                h.responde(s, c, vec![]);
            }
            h
        }
        fn responde(&self, status: u16, corpo: &str, cab: Vec<(&str, &str)>) {
            self.respostas.lock().unwrap().push_back(Resposta {
                status,
                cabecalhos: cab
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                corpo: corpo.as_bytes().to_vec(),
            });
        }
        fn pedidos(&self) -> Vec<Pedido> {
            self.pedidos.lock().unwrap().clone()
        }
    }

    impl Http for &HttpGravado {
        fn enviar(&self, p: Pedido) -> Result<Resposta, ErroNuvem> {
            self.pedidos.lock().unwrap().push(p);
            self.respostas
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| ErroNuvem::Rede("sem resposta gravada".into()))
        }
    }

    #[derive(Default)]
    struct TokenFalso {
        renovado: Mutex<bool>,
    }
    impl Token for &TokenFalso {
        fn atual(&self) -> Result<String, ErroNuvem> {
            Ok(if *self.renovado.lock().unwrap() {
                "t2"
            } else {
                "t1"
            }
            .into())
        }
        fn renovar(&self) -> Result<String, ErroNuvem> {
            *self.renovado.lock().unwrap() = true;
            Ok("t2".into())
        }
    }

    fn arquivo(dir: &std::path::Path, nome: &str, tamanho: usize) -> std::path::PathBuf {
        let p = dir.join(nome);
        std::fs::write(&p, vec![7u8; tamanho]).unwrap();
        p
    }

    fn eh_criar_pasta(p: &Pedido) -> bool {
        p.metodo == "POST"
            && p.url.starts_with(API)
            && String::from_utf8_lossy(&p.corpo).contains("application/vnd.google-apps.folder")
    }

    const VAZIO: &str = r#"{"files":[]}"#;

    #[test]
    fn gdrive_cria_pastas_que_faltam_e_guarda_ids() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        let a = arquivo(t.path(), "a.json", 10);
        let http = HttpGravado::com(vec![
            (200, VAZIO),           // procura "SavesGG Backups"
            (200, r#"{"id":"r"}"#), // cria
            (200, VAZIO),           // procura "SavesGG"
            (200, r#"{"id":"s"}"#),
            (200, VAZIO), // procura "jogo"
            (200, r#"{"id":"j"}"#),
            (200, VAZIO),           // a.json já existe?
            (200, r#"{"id":"f"}"#), // envio multipart
        ]);
        let tok = TokenFalso::default();
        let g = GoogleDrive::new(&http, &tok, ids.clone());
        g.enviar("SavesGG/jogo/a.json", &a).unwrap();
        let p = http.pedidos();
        assert_eq!(p.iter().filter(|x| eh_criar_pasta(x)).count(), 3);
        let up = p.last().unwrap();
        assert!(up.url.contains("uploadType=multipart"), "{}", up.url);
        assert!(String::from_utf8_lossy(&up.corpo).contains(r#""parents":["j"]"#));
        assert!(
            up.cabecalhos
                .iter()
                .any(|(k, v)| k == "Authorization" && v == "Bearer t1")
        );

        // Outra instância usa o cache gravado: nada de listar ou criar pastas.
        let http2 = HttpGravado::com(vec![
            (200, r#"{"trashed":false}"#),
            (200, VAZIO),
            (200, r#"{"id":"g"}"#),
        ]);
        let g2 = GoogleDrive::new(&http2, &tok, ids);
        g2.enviar("SavesGG/jogo/b.json", &a).unwrap();
        assert_eq!(http2.pedidos().len(), 3, "só confere a raiz");
    }

    #[test]
    fn gdrive_envio_existente_usa_patch() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s","SavesGG/jogo":"j"}"#).unwrap();
        let a = arquivo(t.path(), "a.json", 10);
        let http = HttpGravado::com(vec![
            (200, r#"{"trashed":false}"#),
            (
                200,
                r#"{"files":[{"id":"x","name":"a.json","mimeType":"application/json"}]}"#,
            ),
            (200, r#"{"id":"x"}"#),
        ]);
        let tok = TokenFalso::default();
        GoogleDrive::new(&http, &tok, ids)
            .enviar("SavesGG/jogo/a.json", &a)
            .unwrap();
        let up = &http.pedidos()[2];
        assert_eq!(up.metodo, "PATCH");
        assert!(up.url.starts_with(&format!("{UPLOAD}/x?")), "{}", up.url);
    }

    #[test]
    fn gdrive_envio_grande_resumable() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s","SavesGG/jogo":"j"}"#).unwrap();
        let a = arquivo(t.path(), "a.zip", 6 * 1024 * 1024);
        let http = HttpGravado::default();
        http.responde(200, r#"{"trashed":false}"#, vec![]);
        http.responde(200, VAZIO, vec![]);
        http.responde(
            200,
            "",
            vec![("Location", "https://upload.exemplo/sessao1")],
        );
        http.responde(200, r#"{"id":"z"}"#, vec![]);
        let tok = TokenFalso::default();
        GoogleDrive::new(&http, &tok, ids)
            .enviar("SavesGG/jogo/a.zip", &a)
            .unwrap();
        let p = http.pedidos();
        assert!(p[2].url.contains("uploadType=resumable"));
        assert_eq!(p[3].metodo, "PUT");
        assert_eq!(p[3].url, "https://upload.exemplo/sessao1");
        assert_eq!(p[3].corpo.len(), 6 * 1024 * 1024);
    }

    #[test]
    fn gdrive_envio_grande_vai_em_pedacos() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s","SavesGG/jogo":"j"}"#).unwrap();
        let tamanho = 2 * PEDACO + 1000;
        let a = arquivo(t.path(), "a.zip", tamanho);
        let http = HttpGravado::default();
        http.responde(200, r#"{"trashed":false}"#, vec![]); // raiz conferida
        http.responde(200, VAZIO, vec![]);
        http.responde(200, "", vec![("Location", "https://upload.exemplo/s")]);
        http.responde(308, "", vec![]);
        http.responde(308, "", vec![]);
        http.responde(200, r#"{"id":"z"}"#, vec![]);
        let tok = TokenFalso::default();
        GoogleDrive::new(&http, &tok, ids)
            .enviar("SavesGG/jogo/a.zip", &a)
            .unwrap();
        let puts: Vec<Pedido> = http
            .pedidos()
            .into_iter()
            .filter(|p| p.metodo == "PUT")
            .collect();
        assert_eq!(puts.len(), 3);
        let faixa = |p: &Pedido| {
            p.cabecalhos
                .iter()
                .find(|(k, _)| k == "Content-Range")
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(faixa(&puts[0]), format!("bytes 0-{}/{tamanho}", PEDACO - 1));
        assert_eq!(
            faixa(&puts[2]),
            format!("bytes {}-{}/{tamanho}", 2 * PEDACO, tamanho - 1)
        );
        assert_eq!(puts[2].corpo.len(), 1000);
    }

    #[test]
    fn gdrive_raiz_na_lixeira_esquece_o_cache() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"velha","SavesGG":"s-velha"}"#).unwrap();
        let http = HttpGravado::com(vec![
            (200, r#"{"trashed":true}"#), // a raiz do cache está na lixeira
            (200, VAZIO),                 // procura "SavesGG Backups" fora da lixeira
        ]);
        let tok = TokenFalso::default();
        let itens = GoogleDrive::new(&http, &tok, ids.clone())
            .listar("SavesGG")
            .unwrap();
        assert!(itens.is_empty());
        let gravado = std::fs::read_to_string(&ids).unwrap();
        assert!(!gravado.contains("velha"), "{gravado}");
        // num envio, a pasta é criada de novo (fora da lixeira)
        let a = arquivo(t.path(), "a.json", 10);
        let http2 = HttpGravado::com(vec![
            (200, VAZIO),
            (200, r#"{"id":"r2"}"#),
            (200, VAZIO),
            (200, r#"{"id":"s2"}"#),
            (200, VAZIO),
            (200, r#"{"id":"f"}"#),
        ]);
        GoogleDrive::new(&http2, &tok, ids.clone())
            .enviar("SavesGG/a.json", &a)
            .unwrap();
        assert!(
            std::fs::read_to_string(&ids)
                .unwrap()
                .contains(r#""/":"r2""#)
        );
    }

    #[test]
    fn gdrive_sem_permissao_do_drive_e_login() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s"}"#).unwrap();
        let http = HttpGravado::com(vec![
            (200, r#"{"trashed":false}"#),
            (
                403,
                r#"{"error":{"code":403,"errors":[{"reason":"insufficientPermissions"}]}}"#,
            ),
        ]);
        let tok = TokenFalso::default();
        let r = GoogleDrive::new(&http, &tok, ids).listar("SavesGG");
        assert!(matches!(r, Err(ErroNuvem::Login)), "{r:?}");
    }

    #[test]
    fn gdrive_id_sumido_resolve_de_novo() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"velho","SavesGG":"velho2"}"#).unwrap();
        let http = HttpGravado::com(vec![
            (404, r#"{"error":{"code":404}}"#), // listar com id velho
            (
                200,
                r#"{"files":[{"id":"r","name":"SavesGG Backups","mimeType":"application/vnd.google-apps.folder"}]}"#,
            ),
            (
                200,
                r#"{"files":[{"id":"s","name":"SavesGG","mimeType":"application/vnd.google-apps.folder"}]}"#,
            ),
            (
                200,
                r#"{"files":[{"id":"1","name":"hades","mimeType":"application/vnd.google-apps.folder"}]}"#,
            ),
        ]);
        let tok = TokenFalso::default();
        let itens = GoogleDrive::new(&http, &tok, ids.clone())
            .listar("SavesGG")
            .unwrap();
        assert_eq!(itens.len(), 1);
        assert!(itens[0].pasta);
        assert!(
            std::fs::read_to_string(&ids)
                .unwrap()
                .contains(r#""SavesGG":"s""#)
        );
    }

    #[test]
    fn gdrive_listar_usa_query_da_pasta_e_pagina() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s"}"#).unwrap();
        let http = HttpGravado::com(vec![
            (200, r#"{"trashed":false}"#),
            (
                200,
                r#"{"files":[{"id":"1","name":"hades","mimeType":"application/vnd.google-apps.folder"}],"nextPageToken":"p2"}"#,
            ),
            (
                200,
                r#"{"files":[{"id":"2","name":"biblioteca.json","mimeType":"application/json","size":"10"}]}"#,
            ),
        ]);
        let tok = TokenFalso::default();
        let itens = GoogleDrive::new(&http, &tok, ids)
            .listar("SavesGG")
            .unwrap();
        assert_eq!(itens.len(), 2);
        assert_eq!(itens[1].tamanho, 10);
        let p = http.pedidos();
        assert!(
            p[1].url.contains(&codificar("'s' in parents")),
            "{}",
            p[1].url
        );
        assert!(p[1].url.contains("trashed%3Dfalse"));
        assert!(p[2].url.contains("pageToken=p2"));
    }

    #[test]
    fn gdrive_401_renova_token() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s"}"#).unwrap();
        let http = HttpGravado::com(vec![
            (401, "{}"),
            (200, r#"{"trashed":false}"#),
            (200, VAZIO),
        ]);
        let tok = TokenFalso::default();
        GoogleDrive::new(&http, &tok, ids)
            .listar("SavesGG")
            .unwrap();
        let p = http.pedidos();
        assert!(
            p[1].cabecalhos
                .iter()
                .any(|(k, v)| k == "Authorization" && v == "Bearer t2")
        );
    }

    #[test]
    fn gdrive_401_duas_vezes_e_login() {
        let t = tempfile::tempdir().unwrap();
        let ids = t.path().join("ids.json");
        std::fs::write(&ids, r#"{"/":"r","SavesGG":"s"}"#).unwrap();
        let http = HttpGravado::com(vec![
            (200, r#"{"trashed":false}"#),
            (401, "{}"),
            (401, "{}"),
        ]);
        let tok = TokenFalso::default();
        let r = GoogleDrive::new(&http, &tok, ids).listar("SavesGG");
        assert!(matches!(r, Err(ErroNuvem::Login)));
    }

    #[test]
    fn gdrive_pasta_inexistente_lista_vazio() {
        let t = tempfile::tempdir().unwrap();
        let http = HttpGravado::com(vec![(200, VAZIO)]);
        let tok = TokenFalso::default();
        let g = GoogleDrive::new(&http, &tok, t.path().join("ids.json"));
        assert!(g.listar("SavesGG").unwrap().is_empty());
        assert!(
            http.pedidos().iter().all(|p| p.metodo == "GET"),
            "não cria nada só para listar"
        );
    }

    #[test]
    fn gdrive_aspas_no_nome_sao_escapadas() {
        assert_eq!(escapar_q("d'Arc"), r"d\'Arc");
    }
}

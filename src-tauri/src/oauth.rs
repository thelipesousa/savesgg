//! Login OAuth 2.0 com PKCE no navegador do sistema (retorno em 127.0.0.1) e o
//! refresh token guardado no Gerenciador de Credenciais do Windows.
//!
//! Nenhum token vai para log, arquivo, interface ou mensagem de erro.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use savesgg_core::nuvem::http::{Http, HttpUreq, Pedido, Token, codificar};
use savesgg_core::nuvem::{ContaNuvem, ErroNuvem, ProvedorNuvem};

const SERVICO_KEYRING: &str = "SavesGG";
const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const GOOGLE_ESCOPOS: &str = "https://www.googleapis.com/auth/drive.file openid email";
/// Quanto tempo esperar a pessoa terminar o login no navegador.
pub const PRAZO_LOGIN: Duration = Duration::from_secs(5 * 60);

/// Cadastro do app no provedor (os ids entram no build, fora do Git).
#[derive(Debug, Clone, Copy)]
pub struct ClienteOAuth {
    pub provedor: ProvedorNuvem,
    pub client_id: &'static str,
    /// O Google dá um "secret" para apps de desktop que ele mesmo diz não ser confidencial.
    pub client_secret: Option<&'static str>,
}

/// Cadastro do provedor neste build, ou `None` se os ids não vieram no build.
pub fn cliente(p: ProvedorNuvem) -> Option<ClienteOAuth> {
    match p {
        ProvedorNuvem::Gdrive => option_env!("SAVESGG_GOOGLE_CLIENT_ID")
            .filter(|s| !s.trim().is_empty())
            .map(|id| ClienteOAuth {
                provedor: p,
                client_id: id,
                client_secret: option_env!("SAVESGG_GOOGLE_CLIENT_SECRET")
                    .filter(|s| !s.trim().is_empty()),
            }),
    }
}

/// `code_challenge` S256 de um verifier.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// (verifier de 64 caracteres, challenge S256).
pub fn pkce() -> (String, String) {
    const CARACTERES: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::rng();
    let v: String = (0..64)
        .map(|_| CARACTERES[rng.random_range(0..CARACTERES.len())] as char)
        .collect();
    let c = challenge(&v);
    (v, c)
}

fn aleatorio() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

fn redirect(porta: u16) -> String {
    format!("http://127.0.0.1:{porta}")
}

pub fn url_autorizacao(c: &ClienteOAuth, porta: u16, state: &str, challenge: &str) -> String {
    let (autorizacao, escopos) = match c.provedor {
        ProvedorNuvem::Gdrive => (GOOGLE_AUTH, GOOGLE_ESCOPOS),
    };
    let pares = [
        ("client_id", c.client_id.to_string()),
        ("redirect_uri", redirect(porta)),
        ("response_type", "code".into()),
        ("scope", escopos.into()),
        ("code_challenge", challenge.into()),
        ("code_challenge_method", "S256".into()),
        ("state", state.into()),
        ("access_type", "offline".into()),
        ("prompt", "consent".into()),
    ];
    let q: Vec<String> = pares
        .iter()
        .map(|(k, v)| format!("{k}={}", codificar(v)))
        .collect();
    format!("{autorizacao}?{}", q.join("&"))
}

fn decodificar(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parametros(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (decodificar(k), decodificar(v)))
        .collect()
}

const PAGINA_OK: &str = "<!doctype html><meta charset=utf-8><title>SavesGG</title>\
<body style=\"font-family:sans-serif;background:#0d1117;color:#f0f6fc;text-align:center;padding-top:15vh\">\
<h1>Pronto!</h1><p>Pode fechar esta aba e voltar ao SavesGG.</p></body>";
const PAGINA_ERRO: &str = "<!doctype html><meta charset=utf-8><title>SavesGG</title>\
<body style=\"font-family:sans-serif;background:#0d1117;color:#f0f6fc;text-align:center;padding-top:15vh\">\
<h1>Não deu certo</h1><p>Volte ao SavesGG e tente entrar de novo.</p></body>";

fn responder(s: &mut TcpStream, status: &str, html: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = s.flush();
}

/// Lê a linha da requisição (`GET /?... HTTP/1.1`) e devolve o que vem depois do `?`.
fn ler_query(s: &mut TcpStream) -> Option<String> {
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut pedaco = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
        match s.read(&mut pedaco) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&pedaco[..n]),
        }
    }
    let texto = String::from_utf8_lossy(&buf);
    let alvo = texto.lines().next()?.split_whitespace().nth(1)?.to_string();
    Some(
        alvo.split_once('?')
            .map(|(_, q)| q.to_string())
            .unwrap_or_default(),
    )
}

/// Ligada pelo botão Cancelar da tela: o login em andamento para de esperar.
pub static CANCELAR_LOGIN: AtomicBool = AtomicBool::new(false);

/// Espera o navegador voltar para `127.0.0.1` com o código. Confere o `state`.
/// Para se `cancelar` for ligada (a pessoa fechou o navegador e clicou em Cancelar).
pub fn esperar_codigo(
    l: TcpListener,
    state: &str,
    prazo: Duration,
    cancelar: &AtomicBool,
) -> Result<String, String> {
    let fim = Instant::now() + prazo;
    l.set_nonblocking(true)
        .map_err(|_| "não deu para esperar o login".to_string())?;
    while Instant::now() < fim {
        if cancelar.load(Ordering::SeqCst) {
            return Err("login cancelado".into());
        }
        match l.accept() {
            Ok((mut s, _)) => {
                let _ = s.set_nonblocking(false);
                let Some(q) = ler_query(&mut s) else {
                    continue; // conexão vazia (pré-conexão do navegador)
                };
                let p = parametros(&q);
                let valor = |k: &str| p.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
                if valor("code").is_none() && valor("error").is_none() {
                    responder(&mut s, "404 Not Found", PAGINA_ERRO);
                    continue; // favicon e afins
                }
                if valor("state").as_deref() != Some(state) {
                    responder(&mut s, "400 Bad Request", PAGINA_ERRO);
                    return Err("resposta do login inválida; tente de novo".into());
                }
                if valor("error").is_some() {
                    responder(&mut s, "200 OK", PAGINA_ERRO);
                    return Err("o login foi cancelado".into());
                }
                responder(&mut s, "200 OK", PAGINA_OK);
                return Ok(valor("code").unwrap_or_default());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Err("não deu para esperar o login".into()),
        }
    }
    Err("o login foi cancelado ou demorou demais".into())
}

#[derive(Deserialize)]
struct RespostaToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    /// Permissões que a pessoa de fato liberou (o Google deixa desmarcar o Drive).
    scope: Option<String>,
    error: Option<String>,
}

/// A resposta do token libera o Drive? Sem o campo, segue (o envio avisa depois).
fn escopo_tem_drive(scope: Option<&str>) -> bool {
    scope.is_none_or(|s| {
        s.split_whitespace()
            .any(|e| e == "https://www.googleapis.com/auth/drive.file")
    })
}

fn form(pares: &[(&str, &str)]) -> Vec<u8> {
    pares
        .iter()
        .map(|(k, v)| format!("{k}={}", codificar(v)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

fn pedir_token(http: &dyn Http, corpo: Vec<u8>) -> Result<RespostaToken, ErroNuvem> {
    let r = http.enviar(
        Pedido::new("POST", GOOGLE_TOKEN)
            .cabecalho("Content-Type", "application/x-www-form-urlencoded")
            .corpo(corpo),
    )?;
    let t: RespostaToken = serde_json::from_slice(&r.corpo)
        .map_err(|_| ErroNuvem::Servidor(format!("HTTP {}", r.status)))?;
    if t.error.as_deref() == Some("invalid_grant") {
        return Err(ErroNuvem::Login);
    }
    if !(200..300).contains(&r.status) {
        return Err(ErroNuvem::Servidor(format!("HTTP {}", r.status)));
    }
    Ok(t)
}

fn ler_keyring(destino_id: &str) -> Option<String> {
    keyring::Entry::new(SERVICO_KEYRING, destino_id)
        .ok()?
        .get_password()
        .ok()
}

fn gravar_keyring(destino_id: &str, refresh: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICO_KEYRING, destino_id)
        .and_then(|e| e.set_password(refresh))
        .map_err(|_| "não deu para guardar o login no Gerenciador de Credenciais".to_string())
}

/// Apaga o login guardado de uma conta.
pub fn sair(destino_id: &str) {
    if let Ok(e) = keyring::Entry::new(SERVICO_KEYRING, destino_id) {
        let _ = e.delete_credential();
    }
}

/// Token de acesso de uma conta: renova pelo refresh token do Gerenciador de Credenciais.
pub struct TokenKeyring {
    destino_id: String,
    cliente: ClienteOAuth,
    http: Box<dyn Http>,
    ler: fn(&str) -> Option<String>,
    cache: Mutex<Option<(String, Instant)>>,
}

impl TokenKeyring {
    pub fn new(destino_id: String, cliente: ClienteOAuth) -> Self {
        Self::com(
            destino_id,
            cliente,
            Box::new(HttpUreq::default()),
            ler_keyring,
        )
    }

    pub fn com(
        destino_id: String,
        cliente: ClienteOAuth,
        http: Box<dyn Http>,
        ler: fn(&str) -> Option<String>,
    ) -> Self {
        TokenKeyring {
            destino_id,
            cliente,
            http,
            ler,
            cache: Mutex::new(None),
        }
    }
}

impl Token for TokenKeyring {
    fn atual(&self) -> Result<String, ErroNuvem> {
        let guardado = self.cache.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some((t, ate)) = guardado
            && Instant::now() < ate
        {
            return Ok(t);
        }
        self.renovar()
    }

    fn renovar(&self) -> Result<String, ErroNuvem> {
        let refresh = (self.ler)(&self.destino_id).ok_or(ErroNuvem::Login)?;
        let mut pares = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", self.cliente.client_id),
        ];
        if let Some(s) = self.cliente.client_secret {
            pares.push(("client_secret", s));
        }
        let t = pedir_token(self.http.as_ref(), form(&pares))?;
        let acesso = t.access_token.ok_or(ErroNuvem::Login)?;
        if let Some(novo) = t.refresh_token.filter(|r| *r != refresh) {
            let _ = gravar_keyring(&self.destino_id, &novo);
        }
        // Renova um minuto antes de vencer.
        let validade = Duration::from_secs(t.expires_in.unwrap_or(3600).saturating_sub(60));
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((acesso.clone(), Instant::now() + validade));
        Ok(acesso)
    }
}

#[derive(Deserialize)]
struct InfoUsuario {
    sub: String,
    email: Option<String>,
    name: Option<String>,
}

/// Abre a URL no navegador padrão do Windows.
pub fn abrir_navegador(url: &str) {
    let _ = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
}

/// Login completo: abre o navegador, espera o código, troca por tokens, descobre a
/// conta e guarda o refresh token. Erros em texto curto, sem token.
pub fn entrar(p: ProvedorNuvem, abrir: impl Fn(&str)) -> Result<ContaNuvem, String> {
    let c = cliente(p).ok_or("Login na nuvem indisponível nesta versão")?;
    let l = TcpListener::bind("127.0.0.1:0").map_err(|_| "não deu para iniciar o login")?;
    let porta = l
        .local_addr()
        .map_err(|_| "não deu para iniciar o login")?
        .port();
    let (verifier, ch) = pkce();
    let state = aleatorio();
    CANCELAR_LOGIN.store(false, Ordering::SeqCst);
    abrir(&url_autorizacao(&c, porta, &state, &ch));
    let code = esperar_codigo(l, &state, PRAZO_LOGIN, &CANCELAR_LOGIN)?;

    let http = HttpUreq::default();
    let redir = redirect(porta);
    let mut pares = vec![
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redir.as_str()),
        ("client_id", c.client_id),
        ("code_verifier", verifier.as_str()),
    ];
    if let Some(s) = c.client_secret {
        pares.push(("client_secret", s));
    }
    let t = pedir_token(&http, form(&pares)).map_err(|e| format!("o login falhou: {e}"))?;
    if !escopo_tem_drive(t.scope.as_deref()) {
        return Err("Marque a permissão do Google Drive na tela do Google e entre de novo".into());
    }
    let acesso = t
        .access_token
        .ok_or("o login falhou: sem token de acesso")?;
    let refresh = t
        .refresh_token
        .ok_or("o Google não liberou o acesso contínuo; tente entrar de novo")?;

    let r = http
        .enviar(
            Pedido::new("GET", GOOGLE_USERINFO)
                .cabecalho("Authorization", format!("Bearer {acesso}")),
        )
        .map_err(|e| format!("o login falhou: {e}"))?;
    let info: InfoUsuario =
        serde_json::from_slice(&r.corpo).map_err(|_| "o login falhou: conta não identificada")?;
    let conta = ContaNuvem {
        provedor: p,
        id: info.sub,
        email: info.email.unwrap_or_default(),
        nome: info.name.unwrap_or_default(),
    };
    gravar_keyring(&conta.destino_id(), &refresh)?;
    log::info!("nuvem: conta conectada ({})", conta.provedor.nome());
    Ok(conta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use savesgg_core::nuvem::http::{Pedido, Resposta};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::atomic::AtomicBool;

    fn cliente_teste() -> ClienteOAuth {
        ClienteOAuth {
            provedor: ProvedorNuvem::Gdrive,
            client_id: "id-teste.apps.googleusercontent.com",
            client_secret: Some("segredo-teste"),
        }
    }

    #[test]
    fn pkce_challenge_confere() {
        // Exemplo da RFC 7636, apêndice B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let (v, c) = pkce();
        assert_eq!(v.len(), 64);
        assert_eq!(challenge(&v), c);
    }

    #[test]
    fn url_google_usa_127() {
        let u = url_autorizacao(&cliente_teste(), 51234, "s1", "c1");
        assert!(u.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(u.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A51234"));
        assert!(u.contains("response_type=code"));
        assert!(u.contains("code_challenge=c1"));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains("state=s1"));
        assert!(u.contains("access_type=offline"));
        assert!(u.contains("prompt=consent"));
        assert!(u.contains(&codificar("https://www.googleapis.com/auth/drive.file")));
    }

    fn chamar_loopback(porta: u16, caminho: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", porta)).unwrap();
        write!(s, "GET {caminho} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut r = String::new();
        let _ = s.read_to_string(&mut r);
        r
    }

    #[test]
    fn loopback_devolve_codigo() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || chamar_loopback(porta, "/?code=abc&state=s1&scope=x"));
        let r = esperar_codigo(l, "s1", Duration::from_secs(5), &AtomicBool::new(false));
        assert_eq!(r, Ok("abc".to_string()));
        assert!(t.join().unwrap().contains("Pode fechar esta aba"));
    }

    #[test]
    fn loopback_state_errado_falha() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || chamar_loopback(porta, "/?code=abc&state=outro"));
        assert!(esperar_codigo(l, "s1", Duration::from_secs(5), &AtomicBool::new(false)).is_err());
        t.join().unwrap();
    }

    #[test]
    fn loopback_cancelado_falha() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        let t =
            std::thread::spawn(move || chamar_loopback(porta, "/?error=access_denied&state=s1"));
        let r = esperar_codigo(l, "s1", Duration::from_secs(5), &AtomicBool::new(false));
        assert!(r.unwrap_err().contains("cancelado"));
        t.join().unwrap();
    }

    #[test]
    fn loopback_cancelado_pelo_app() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cancelar = AtomicBool::new(true);
        let r = esperar_codigo(l, "s1", Duration::from_secs(5), &cancelar);
        assert_eq!(r, Err("login cancelado".to_string()));
    }

    #[test]
    fn escopo_precisa_do_drive() {
        assert!(escopo_tem_drive(Some(
            "openid https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/userinfo.email"
        )));
        assert!(!escopo_tem_drive(Some(
            "openid https://www.googleapis.com/auth/userinfo.email"
        )));
        // Sem o campo na resposta, não dá para saber: segue (o envio avisa depois).
        assert!(escopo_tem_drive(None));
    }

    #[test]
    fn loopback_timeout() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let r = esperar_codigo(l, "s1", Duration::from_millis(200), &AtomicBool::new(false));
        assert!(r.unwrap_err().contains("demorou demais"));
    }

    struct HttpFixo(u16, &'static str);
    impl Http for HttpFixo {
        fn enviar(&self, _p: Pedido) -> Result<Resposta, ErroNuvem> {
            Ok(Resposta {
                status: self.0,
                cabecalhos: vec![],
                corpo: self.1.as_bytes().to_vec(),
            })
        }
    }

    #[test]
    fn token_revogado_vira_entrar_de_novo() {
        let t = TokenKeyring::com(
            "nuvem:gdrive:abc".into(),
            cliente_teste(),
            Box::new(HttpFixo(400, r#"{"error":"invalid_grant"}"#)),
            |_| Some("refresh-velho".into()),
        );
        assert!(matches!(t.renovar(), Err(ErroNuvem::Login)));
    }

    #[test]
    fn token_renovado_fica_em_cache() {
        let t = TokenKeyring::com(
            "nuvem:gdrive:abc".into(),
            cliente_teste(),
            Box::new(HttpFixo(200, r#"{"access_token":"a1","expires_in":3600}"#)),
            |_| Some("r".into()),
        );
        assert_eq!(t.atual().unwrap(), "a1");
        assert_eq!(t.atual().unwrap(), "a1");
    }

    #[test]
    fn sem_refresh_guardado_e_login() {
        let t = TokenKeyring::com(
            "nuvem:gdrive:abc".into(),
            cliente_teste(),
            Box::new(HttpFixo(200, "{}")),
            |_| None,
        );
        assert!(matches!(t.atual(), Err(ErroNuvem::Login)));
    }
}

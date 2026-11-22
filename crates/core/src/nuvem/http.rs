//! HTTP mínimo para os provedores de nuvem (testável com respostas gravadas).

use std::time::Duration;

use super::ErroNuvem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    pub metodo: &'static str,
    pub url: String,
    pub cabecalhos: Vec<(String, String)>,
    pub corpo: Vec<u8>,
}

impl Pedido {
    pub fn new(metodo: &'static str, url: impl Into<String>) -> Self {
        Pedido {
            metodo,
            url: url.into(),
            cabecalhos: Vec::new(),
            corpo: Vec::new(),
        }
    }

    pub fn cabecalho(mut self, nome: &str, valor: impl Into<String>) -> Self {
        self.cabecalhos.push((nome.to_string(), valor.into()));
        self
    }

    pub fn corpo(mut self, corpo: Vec<u8>) -> Self {
        self.corpo = corpo;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resposta {
    pub status: u16,
    pub cabecalhos: Vec<(String, String)>,
    pub corpo: Vec<u8>,
}

impl Resposta {
    /// Valor de um cabeçalho, sem diferenciar maiúsculas.
    pub fn cabecalho(&self, nome: &str) -> Option<&str> {
        self.cabecalhos
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(nome))
            .map(|(_, v)| v.as_str())
    }

    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Faz uma requisição. Erro só quando nem houve resposta (rede); status HTTP de erro
/// volta como `Resposta` para quem chamou decidir.
pub trait Http: Send + Sync {
    fn enviar(&self, p: Pedido) -> Result<Resposta, ErroNuvem>;
}

/// Token de acesso com renovação (o refresh token fica no Gerenciador de Credenciais).
pub trait Token: Send + Sync {
    fn atual(&self) -> Result<String, ErroNuvem>;
    fn renovar(&self) -> Result<String, ErroNuvem>;
}

/// Tamanho máximo de resposta lida para a memória (um save maior que isso é "grande").
const LIMITE_CORPO: u64 = 4 * 1024 * 1024 * 1024;

/// `Http` de verdade, com ureq.
pub struct HttpUreq {
    agente: ureq::Agent,
}

impl Default for HttpUreq {
    fn default() -> Self {
        let agente: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(30 * 60)))
            .build()
            .into();
        HttpUreq { agente }
    }
}

impl Http for HttpUreq {
    fn enviar(&self, p: Pedido) -> Result<Resposta, ErroNuvem> {
        let mut req = ureq::http::Request::builder().method(p.metodo).uri(&p.url);
        for (k, v) in &p.cabecalhos {
            req = req.header(k, v);
        }
        let req = req
            .body(p.corpo)
            .map_err(|e| ErroNuvem::Local(format!("requisição inválida: {e}")))?;
        let mut resp = self
            .agente
            .run(req)
            .map_err(|e| ErroNuvem::Rede(e.to_string()))?;
        let status = resp.status().as_u16();
        let cabecalhos = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let corpo = resp
            .body_mut()
            .with_config()
            .limit(LIMITE_CORPO)
            .read_to_vec()
            .map_err(|e| ErroNuvem::Rede(e.to_string()))?;
        Ok(Resposta {
            status,
            cabecalhos,
            corpo,
        })
    }
}

/// Codifica um valor para ir na URL (query ou caminho).
pub fn codificar(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codificar_escapa_reservados() {
        assert_eq!(codificar("a b'c/d"), "a%20b%27c%2Fd");
        assert_eq!(codificar("ção"), "%C3%A7%C3%A3o");
    }

    #[test]
    fn cabecalho_sem_diferenciar_maiusculas() {
        let r = Resposta {
            status: 200,
            cabecalhos: vec![("Location".into(), "x".into())],
            corpo: vec![],
        };
        assert_eq!(r.cabecalho("location"), Some("x"));
    }
}

//! Heurística para saves de jogos que o manifesto não conhece.

use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::paths::PathContext;

/// Pastas de primeiro nível que nunca são jogos.
const IGNORAR: &[&str] = &[
    "microsoft",
    "google",
    "mozilla",
    "nvidia corporation",
    "packages",
    "temp",
    "intel",
    "amd",
    "adobe",
    "apple",
    "discord",
    "code",
    "npm",
    "nodejs",
    "python",
    "jetbrains",
    "spotify",
    "steam",
    "epic",
    "gog.com",
    "obs-studio",
    "zoom",
    "slack",
    "whatsapp",
    "telegram desktop",
    "ebwebview",
    "webview",
    "cache",
    "gpucache",
    "crashdumps",
    "crashes",
    "logs",
    "unity",
];

/// Um candidato: (nome sugerido, pasta).
pub struct Candidate {
    pub name: String,
    pub dir: PathBuf,
}

fn ignorada(name: &str) -> bool {
    let n = name.to_lowercase();
    IGNORAR.iter().any(|i| *i == n)
}

/// `strict` = só arquivos com "save" no nome ou extensões fortes (sem `.dat`).
fn parece_save(file: &Path, strict: bool) -> bool {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let forte = matches!(ext.as_str(), "sav" | "save" | "sl2" | "ess") || name.contains("save");
    forte || (!strict && ext == "dat")
}

fn tem_save(dir: &Path, strict: bool) -> bool {
    WalkDir::new(dir)
        .max_depth(4)
        .into_iter()
        .flatten()
        .any(|e| e.file_type().is_file() && parece_save(e.path(), strict))
}

fn subpastas(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

/// Pastas com cara de save nos lugares típicos.
pub fn candidates(ctx: &PathContext) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut push = |dir: PathBuf, strict: bool| {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !ignorada(&name) && tem_save(&dir, strict) {
            out.push(Candidate { name, dir });
        }
    };
    for d in subpastas(&ctx.documents.join("My Games")) {
        push(d, false);
    }
    for d in subpastas(&ctx.home.join("Saved Games")) {
        push(d, false);
    }
    for company in subpastas(&ctx.local_app_data_low) {
        let cname = company
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if ignorada(&cname) {
            continue;
        }
        for d in subpastas(&company) {
            push(d, false);
        }
    }
    for d in subpastas(&ctx.app_data) {
        push(d, true);
    }
    out
}

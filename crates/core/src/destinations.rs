//! Destinos de backup: pastas sincronizadas, discos externos e pastas escolhidas.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backup::backup_root;
use crate::paths::norm;
use crate::sync_folders::{SyncProvider, SyncRoot};

/// Subpasta criada dentro das pastas sincronizadas.
pub const PASTA_NA_NUVEM: &str = "SaveGuard Backups";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", content = "provedor", rename_all = "snake_case")]
pub enum DestinationKind {
    PastaSincronizada(SyncProvider),
    DiscoRemovivel,
    PastaLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Destination {
    /// Estável entre execuções (discos usam o número de série do volume, não a letra).
    pub id: String,
    pub label: String,
    pub kind: DestinationKind,
    pub root: PathBuf,
    pub available: bool,
    pub free_bytes: Option<u64>,
    pub has_backup: bool,
}

/// Um disco externo plugado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveInfo {
    pub root: PathBuf,
    pub label: String,
    pub serial: u32,
    pub free_bytes: Option<u64>,
}

fn has_backup(root: &Path) -> bool {
    backup_root(root).join("biblioteca.json").is_file()
}

fn provider_label(p: SyncProvider) -> &'static str {
    match p {
        SyncProvider::OneDrive => "OneDrive",
        SyncProvider::GoogleDrive => "Google Drive",
        SyncProvider::Dropbox => "Dropbox",
    }
}

/// Monta a lista de destinos a partir do que foi detectado (função pura, testável).
pub fn build_destinations(
    sync: &[SyncRoot],
    drives: &[DriveInfo],
    custom: &[PathBuf],
) -> Vec<Destination> {
    let mut out = Vec::new();
    for s in sync {
        let base = provider_label(s.provider);
        // "OneDrive - Empresa" → "OneDrive (Empresa)"; a pasta pessoal fica só "OneDrive".
        let folder = s
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let label = match folder.split_once(" - ") {
            Some((_, org)) => format!("{base} ({org})"),
            None => base.to_string(),
        };
        let root = s.path.join(PASTA_NA_NUVEM);
        out.push(Destination {
            id: format!("sync:{}", norm(&s.path).to_lowercase()),
            label,
            kind: DestinationKind::PastaSincronizada(s.provider),
            available: s.path.is_dir(),
            free_bytes: None,
            has_backup: has_backup(&root),
            root,
        });
    }
    for d in drives {
        let letter = norm(&d.root);
        let name = if d.label.is_empty() {
            "Disco externo".to_string()
        } else {
            d.label.clone()
        };
        out.push(Destination {
            id: format!("disco:{:08x}", d.serial),
            label: format!("{name} ({letter})"),
            kind: DestinationKind::DiscoRemovivel,
            available: d.root.is_dir(),
            free_bytes: d.free_bytes,
            has_backup: has_backup(&d.root),
            root: d.root.clone(),
        });
    }
    for c in custom {
        out.push(Destination {
            id: format!("pasta:{}", norm(c).to_lowercase()),
            label: c
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| norm(c)),
            kind: DestinationKind::PastaLocal,
            available: c.is_dir(),
            free_bytes: None,
            has_backup: has_backup(c),
            root: c.clone(),
        });
    }
    out
}

/// Todos os destinos deste PC: pastas sincronizadas, discos externos e pastas da configuração.
#[cfg(windows)]
pub fn list_destinations(cfg: &crate::config::Config) -> Vec<Destination> {
    build_destinations(
        &crate::sync_folders::detect_sync_roots(),
        &win::external_drives(),
        &cfg.custom_folders,
    )
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BusTypeUsb, CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW,
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
    };

    use super::DriveInfo;

    /// Valores de `GetDriveTypeW` (WinBase.h).
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Disco fixo ligado por USB (HD externo).
    fn is_usb(letter: char) -> bool {
        let path = wide(&format!("\\\\.\\{letter}:"));
        // SAFETY: chamadas Win32 com buffers locais de tamanho correto; o handle é fechado.
        unsafe {
            let h = CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if h == INVALID_HANDLE_VALUE {
                return false;
            }
            let query = STORAGE_PROPERTY_QUERY {
                PropertyId: StorageDeviceProperty,
                QueryType: PropertyStandardQuery,
                AdditionalParameters: [0],
            };
            let mut buf = [0u8; 1024];
            let mut ret = 0u32;
            let ok = DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &query as *const _ as *const _,
                size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut ret,
                std::ptr::null_mut(),
            );
            CloseHandle(h);
            if ok == 0 || (ret as usize) < size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
                return false;
            }
            let desc = std::ptr::read_unaligned(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
            desc.BusType == BusTypeUsb
        }
    }

    /// Pendrives e HDs externos plugados agora (nunca o disco do sistema).
    pub fn external_drives() -> Vec<DriveInfo> {
        let system = std::env::var("SystemDrive")
            .ok()
            .and_then(|s| s.chars().next())
            .unwrap_or('C')
            .to_ascii_uppercase();
        // SAFETY: GetLogicalDrives não recebe ponteiros.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            if letter == system {
                continue;
            }
            let root = format!("{letter}:\\");
            let w = wide(&root);
            // SAFETY: `w` é uma string UTF-16 terminada em zero.
            let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
            let external = kind == DRIVE_REMOVABLE || (kind == DRIVE_FIXED && is_usb(letter));
            if !external {
                continue;
            }
            let mut name = [0u16; 261];
            let mut serial = 0u32;
            let mut free = 0u64;
            // SAFETY: buffers locais com os tamanhos informados.
            let (has_vol, has_free) = unsafe {
                let v = GetVolumeInformationW(
                    w.as_ptr(),
                    name.as_mut_ptr(),
                    name.len() as u32,
                    &mut serial,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                );
                let f = GetDiskFreeSpaceExW(
                    w.as_ptr(),
                    &mut free,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                (v != 0, f != 0)
            };
            if !has_vol {
                continue; // leitor de cartão vazio etc.
            }
            let len = name.iter().position(|&c| c == 0).unwrap_or(0);
            out.push(DriveInfo {
                root: PathBuf::from(&root),
                label: String::from_utf16_lossy(&name[..len]),
                serial,
                free_bytes: has_free.then_some(free),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_folders::{SyncProvider, SyncRoot};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn pasta_sincronizada_vira_destino() {
        let t = tempfile::tempdir().unwrap();
        let od = t.path().join("OneDrive");
        fs::create_dir_all(&od).unwrap();
        let sync = [SyncRoot {
            provider: SyncProvider::OneDrive,
            path: od.clone(),
        }];
        let d = build_destinations(&sync, &[], &[]);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].kind,
            DestinationKind::PastaSincronizada(SyncProvider::OneDrive)
        );
        assert_eq!(d[0].root, od.join("SaveGuard Backups"));
        assert_eq!(d[0].label, "OneDrive");
        assert!(d[0].available);
        assert!(!d[0].has_backup);
        assert!(d[0].id.starts_with("sync:"));
    }

    #[test]
    fn rotulo_de_onedrive_da_organizacao() {
        let t = tempfile::tempdir().unwrap();
        let org = t.path().join("OneDrive - Faculdade X");
        let pessoal = t.path().join("OneDrive");
        let sync = [
            SyncRoot {
                provider: SyncProvider::OneDrive,
                path: org,
            },
            SyncRoot {
                provider: SyncProvider::OneDrive,
                path: pessoal,
            },
        ];
        let labels: Vec<_> = build_destinations(&sync, &[], &[])
            .into_iter()
            .map(|d| d.label)
            .collect();
        assert_eq!(labels, ["OneDrive (Faculdade X)", "OneDrive"]);
    }

    #[test]
    fn disco_removivel_usa_serial_no_id() {
        let t = tempfile::tempdir().unwrap();
        let drives = [DriveInfo {
            root: t.path().to_path_buf(),
            label: "PENDRIVE".into(),
            serial: 0xABCD_1234,
            free_bytes: Some(1_000),
        }];
        let d = build_destinations(&[], &drives, &[]);
        assert_eq!(d[0].id, "disco:abcd1234");
        assert_eq!(d[0].kind, DestinationKind::DiscoRemovivel);
        assert_eq!(d[0].free_bytes, Some(1_000));
        assert!(d[0].label.contains("PENDRIVE"));
    }

    #[test]
    fn pasta_custom_marca_has_backup() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("SaveGuard")).unwrap();
        fs::write(t.path().join("SaveGuard/biblioteca.json"), "{}").unwrap();
        let d = build_destinations(&[], &[], &[t.path().to_path_buf()]);
        assert_eq!(d[0].kind, DestinationKind::PastaLocal);
        assert!(d[0].has_backup);
        assert!(d[0].available);
    }

    #[test]
    fn pasta_custom_inexistente_indisponivel() {
        let d = build_destinations(&[], &[], &[PathBuf::from("Z:/sumiu")]);
        assert!(!d[0].available);
    }

    #[test]
    fn id_de_pasta_ignora_maiusculas() {
        let a = build_destinations(&[], &[], &[PathBuf::from(r"D:\Backups")]);
        let b = build_destinations(&[], &[], &[PathBuf::from("d:/backups")]);
        assert_eq!(a[0].id, b[0].id);
    }
}

#[cfg(all(test, windows))]
mod pc_real {
    /// `cargo test -- --ignored destinos_reais --nocapture`
    #[test]
    #[ignore]
    fn destinos_reais() {
        for d in super::list_destinations(&crate::config::Config::default()) {
            eprintln!(
                "{} | {} | {:?} | disponível={} | backup={}",
                d.id, d.label, d.kind, d.available, d.has_backup
            );
        }
    }
}

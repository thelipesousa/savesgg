//! Backup contínuo: agendador, processos e pastas vigiadas.

pub mod scheduler;

pub use scheduler::{ESPERA_APOS_FECHAR, SILENCIO_DA_PASTA, Scheduler, WatchEvent};

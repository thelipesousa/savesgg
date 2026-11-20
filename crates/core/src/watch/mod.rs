//! Backup contínuo: agendador, processos e pastas vigiadas.

pub mod folders;
pub mod process;
pub mod scheduler;

pub use folders::{games_for_path, watch_roots};
pub use process::{ProcInfo, ProcessTracker};
pub use scheduler::{ESPERA_APOS_FECHAR, SILENCIO_DA_PASTA, Scheduler, WatchEvent};

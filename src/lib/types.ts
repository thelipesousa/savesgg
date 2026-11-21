// Espelho dos tipos enviados pelos comandos Tauri (src-tauri/src/views.rs e commands.rs).

export type Store = "steam" | "epic" | "gog" | "xbox" | "emulador" | "sem_loja";
export type Source = "manifesto" | "emulador" | "heuristica" | "manual";
export type Priority = "alta" | "media" | "baixa";

export interface GameView {
  id: string;
  name: string;
  store: Store;
  source: Source;
  priority: Priority;
  protections: string[];
  files: number;
  total_bytes: number;
  last_backup: string | null;
  save_roots: string[];
  grande: boolean;
  confirmado: boolean;
  cover_url: string | null;
}

export type DestinationKind =
  | { tipo: "pasta_sincronizada"; provedor: "one_drive" | "google_drive" | "dropbox" }
  | { tipo: "disco_removivel" }
  | { tipo: "pasta_local" };

export interface Destination {
  id: string;
  label: string;
  kind: DestinationKind;
  root: string;
  available: boolean;
  free_bytes: number | null;
  has_backup: boolean;
}

export interface BackupResultView {
  game_id: string;
  destination_id: string;
  status: "criado" | "sem_mudanca" | "erro";
  message: string | null;
}

export interface SnapshotView {
  stem: string;
  created: string;
  pinned: boolean;
  total_bytes: number;
  seguranca: boolean;
}

export interface GameSnapshotsView {
  game_id: string;
  game_name: string;
  snapshots: SnapshotView[];
  keep?: number | null;
}

export interface RestoreItem {
  zip_path: string;
  template: string;
  target: string | null;
  exists: boolean;
  existing_modified: string | null;
}

export interface RestorePlan {
  game_id: string;
  game_name: string;
  stem: string;
  items: RestoreItem[];
  unresolved: number;
  conflicts: number;
}

export interface RestoreReport {
  restored: number;
  safety_snapshot: { stem: string } | null;
}

export interface SettingsView {
  auto_destinations: string[];
  autostart: boolean;
  auto_paused: boolean;
  keep_versions: number;
  theme: string;
  game_view: "lista" | "grade";
  wizard_done: boolean;
}

export interface AutoStatusView {
  paused: boolean;
  running: string[];
  waiting: number;
  pending: number;
  last_backup: string | null;
  at_risk: number;
  copying: boolean;
  last_error: string | null;
}

export type LoteResultado =
  | { tipo: "restaurado"; arquivos: number; seguranca: string | null }
  | { tipo: "precisa_de_pasta"; faltando: number }
  | { tipo: "erro"; mensagem: string };

export interface LoteItemView {
  game_id: string;
  game_name: string;
  resultado: LoteResultado;
}

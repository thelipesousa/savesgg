import { invoke } from "@tauri-apps/api/core";
import type {
  BackupResultView,
  Destination,
  GameSnapshotsView,
  GameView,
  IgnoredView,
  LoteItemView,
  RestorePlan,
  RestoreReport,
  SettingsView,
  AutoStatusView,
} from "./types";

export const api = {
  scan: () => invoke<GameView[]>("scan"),
  cachedScan: () => invoke<GameView[] | null>("cached_scan"),
  listDestinations: () => invoke<Destination[]>("list_destinations_cmd"),
  addCustomFolder: () => invoke<Destination | null>("add_custom_folder"),
  pickFolder: () => invoke<string | null>("pick_folder"),
  backupNow: (gameIds: string[], destinationIds: string[]) =>
    invoke<BackupResultView[]>("backup_now", { gameIds, destinationIds }),
  listSnapshots: (destinationId: string) =>
    invoke<GameSnapshotsView[]>("list_snapshots", { destinationId }),
  planRestore: (destinationId: string, gameId: string, stem: string) =>
    invoke<RestorePlan>("plan_restore", { destinationId, gameId, stem }),
  applyRestore: (destinationId: string, gameId: string, stem: string, overrides: Record<string, string>) =>
    invoke<RestoreReport>("apply_restore", { destinationId, gameId, stem, overrides }),
  confirmHeuristic: (gameId: string, keep: boolean) =>
    invoke<void>("confirm_heuristic", { gameId, keep }),
  appVersion: () => invoke<string>("app_version"),
  getSettings: () => invoke<SettingsView>("get_settings"),
  setAutoDestinations: (ids: string[]) => invoke<void>("set_auto_destinations", { ids }),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  autoStatus: () => invoke<AutoStatusView>("auto_status"),
  setTheme: (theme: string) => invoke<void>("set_theme", { theme }),
  setGameView: (view: string) => invoke<void>("set_game_view", { view }),
  completeWizard: () => invoke<void>("complete_wizard"),
  openBackupFolder: () => invoke<Destination | null>("open_backup_folder"),
  setPinned: (destinationId: string, gameId: string, stem: string, pinned: boolean) =>
    invoke<void>("set_pinned_cmd", { destinationId, gameId, stem, pinned }),
  setKeepVersions: (n: number) => invoke<void>("set_keep_versions", { n }),
  setGameKeep: (gameId: string, n: number | null) => invoke<void>("set_game_keep", { gameId, n }),
  addManualGame: (name: string) => invoke<GameView[] | null>("add_manual_game", { name }),
  removeManualGame: (gameId: string) => invoke<void>("remove_manual_game", { gameId }),
  ignoredGames: () => invoke<IgnoredView[]>("ignored_games"),
  restoreMany: (destinationId: string, gameIds: string[]) =>
    invoke<LoteItemView[]>("restore_many", { destinationId, gameIds }),
  copyAllTo: (destinationId: string) => invoke<BackupResultView[]>("copy_all_to", { destinationId }),
};

/** Mensagem de erro amigável a partir do que o comando devolveu. */
export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Algo deu errado.";
}

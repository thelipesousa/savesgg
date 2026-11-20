import { invoke } from "@tauri-apps/api/core";
import type {
  BackupResultView,
  Destination,
  GameSnapshotsView,
  GameView,
  RestorePlan,
  RestoreReport,
} from "./types";

export const api = {
  scan: () => invoke<GameView[]>("scan"),
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
};

/** Mensagem de erro amigável a partir do que o comando devolveu. */
export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Algo deu errado.";
}

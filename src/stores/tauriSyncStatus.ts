import { listen } from "@tauri-apps/api/event";
import type { SyncStatusChanged } from "../types/contracts";
import type { SyncStatusListenPort } from "./syncStatusStore";

export const listenSyncStatus: SyncStatusListenPort = (listener) => (
  listen<SyncStatusChanged>("sync_status_changed", ({ payload }) => listener(payload))
);

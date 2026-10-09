import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppSnapshot } from "../types/contracts";
import type { AppSnapshotListenPort, AppSnapshotPort } from "./appSnapshotHydration";

export const invokeAppSnapshot: AppSnapshotPort = () => invoke<AppSnapshot>("get_app_snapshot");

export const listenAppSnapshot: AppSnapshotListenPort = (listener) => (
  listen<AppSnapshot>("snapshot_changed", ({ payload }) => listener(payload))
);

import { invoke } from "@tauri-apps/api/core";
import type { OpenAccountPort } from "./openAccountController";

export const invokeOpenAccount: OpenAccountPort = (accountId) => (
  invoke<void>("open_account", { accountId })
);

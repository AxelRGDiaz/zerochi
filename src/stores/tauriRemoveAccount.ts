import { invoke } from "@tauri-apps/api/core";
import type { RemoveAccountResult } from "../types/contracts";
import type { RemoveAccountPort } from "./removeAccountController";

export const invokeRemoveAccount: RemoveAccountPort = (accountId) => (
  invoke<RemoveAccountResult>("remove_account", { accountId })
);

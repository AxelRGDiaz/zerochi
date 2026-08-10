import { invoke } from "@tauri-apps/api/core";
import type { AddAccountResult } from "../types/contracts";
import type { AddAccountPort } from "./addAccountController";

export const invokeAddAccount: AddAccountPort = () => invoke<AddAccountResult>("add_account");

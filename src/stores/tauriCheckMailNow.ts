import { invoke } from "@tauri-apps/api/core";
import type { CheckMailNowPort } from "./checkMailNowController";

export const invokeCheckMailNow: CheckMailNowPort = async () => {
  await invoke<unknown>("check_mail_now");
};

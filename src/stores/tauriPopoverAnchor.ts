import { listen } from "@tauri-apps/api/event";
import type { PopoverAnchorChanged } from "../types/contracts";
import type { PopoverAnchorListenPort } from "./popoverAnchor";

export const listenPopoverAnchor: PopoverAnchorListenPort = (listener) => (
  listen<PopoverAnchorChanged>("popover_anchor_changed", ({ payload }) => listener(payload))
);

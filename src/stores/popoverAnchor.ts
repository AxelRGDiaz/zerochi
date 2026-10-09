import type { PopoverAnchorChanged } from "../types/contracts";

export type PopoverAnchorListener = (x: number) => void;
export type PopoverAnchorListenPort = (
  listener: (event: PopoverAnchorChanged) => void,
) => Promise<() => void>;

export interface PopoverAnchorSubscription {
  start(): () => void;
}

export function createPopoverAnchorSubscription(
  listenPopoverAnchor: PopoverAnchorListenPort,
  applyAnchor: PopoverAnchorListener,
): PopoverAnchorSubscription {
  return {
    start() {
      let disposed = false;
      let unlisten: (() => void) | null = null;

      try {
        void listenPopoverAnchor((event) => {
          if (!disposed && Number.isFinite(event?.x)) applyAnchor(event.x);
        })
          .then((stopListening) => {
            if (disposed) stopListening();
            else unlisten = stopListening;
          })
          .catch(() => undefined);
      } catch {
        // Keep the centered CSS default when native anchor registration fails.
      }

      return () => {
        disposed = true;
        unlisten?.();
        unlisten = null;
      };
    },
  };
}

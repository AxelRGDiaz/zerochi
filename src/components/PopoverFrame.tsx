import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPopoverAnchorSubscription } from "../stores/popoverAnchor";
import { listenPopoverAnchor } from "../stores/tauriPopoverAnchor";

interface PopoverFrameProps {
  children: ReactNode;
}

export const POPOVER_POINTER_SAFE_INSET = 28;

export function safePopoverAnchorX(
  windowX: number,
  frameLeft: number,
  frameWidth: number,
): number | null {
  if (![windowX, frameLeft, frameWidth].every(Number.isFinite) || frameWidth <= 0) return null;
  const midpoint = frameWidth / 2;
  const minimum = Math.min(POPOVER_POINTER_SAFE_INSET, midpoint);
  const maximum = Math.max(minimum, frameWidth - POPOVER_POINTER_SAFE_INSET);
  return Math.min(maximum, Math.max(minimum, windowX - frameLeft));
}

export function PopoverFrame({ children }: PopoverFrameProps) {
  const frameRef = useRef<HTMLDivElement | null>(null);
  const [anchorX, setAnchorX] = useState<number | null>(null);

  useEffect(() => createPopoverAnchorSubscription(listenPopoverAnchor, (windowX) => {
    const bounds = frameRef.current?.getBoundingClientRect();
    if (!bounds) return;
    const safeAnchor = safePopoverAnchorX(windowX, bounds.left, bounds.width);
    if (safeAnchor !== null) setAnchorX(safeAnchor);
  }).start(), []);

  const style = anchorX === null
    ? undefined
    : { "--popover-anchor-x": `${anchorX}px` } as CSSProperties;

  return <div className="popover-frame" ref={frameRef} style={style}>{children}</div>;
}

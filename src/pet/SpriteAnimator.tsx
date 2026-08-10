import { useEffect, useState } from "react";
import type { SpriteAnimationDefinition } from "./spriteManifest";

interface Props {
  definition: SpriteAnimationDefinition;
  frameSize: number;
  imageUrl: string;
  scale?: 1 | 2 | 3 | 4;
  visible: boolean;
  onComplete?: () => void;
}

export function SpriteAnimator({ definition, frameSize, imageUrl, scale = 3, visible, onComplete }: Props) {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    setFrame(0);
    if (!visible || definition.frameCount <= 1) return;
    const timer = window.setInterval(() => {
      setFrame((current) => {
        const next = current + 1;
        if (next < definition.frameCount) return next;
        if (definition.loop) return 0;
        window.clearInterval(timer);
        onComplete?.();
        return current;
      });
    }, 1000 / definition.fps);
    return () => window.clearInterval(timer);
  }, [definition, onComplete, visible]);

  if (!visible) return null;
  const x = -(definition.startFrame + frame) * frameSize;
  const y = -definition.row * frameSize;
  return (
    <div role="img" aria-label={definition.id} style={{ width: frameSize * scale, height: frameSize * scale, overflow: "hidden" }}>
      <div style={{
        width: frameSize,
        height: frameSize,
        backgroundImage: `url(${imageUrl})`,
        backgroundPosition: `${x}px ${y}px`,
        imageRendering: "pixelated",
        transform: `scale(${scale})`,
        transformOrigin: "top left",
      }} />
    </div>
  );
}

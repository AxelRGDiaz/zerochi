import { useEffect, useRef, type ReactNode } from "react";

interface ViewHeadingProps {
  children: ReactNode;
  id?: string;
}

export function focusViewHeading(target: Pick<HTMLElement, "focus"> | null): void {
  target?.focus();
}

export function ViewHeading({ children, id }: ViewHeadingProps) {
  const headingRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    focusViewHeading(headingRef.current);
  }, []);

  return <h1 id={id} ref={headingRef} tabIndex={-1}>{children}</h1>;
}

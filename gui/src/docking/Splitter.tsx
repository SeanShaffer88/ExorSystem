import { useRef, type RefObject } from "react";
import { dockName, splitSizes, type SplitArea } from "./layout";

export function Splitter({ area, value, container, controls, onResize }: {
  area: SplitArea; value: number; container: RefObject<HTMLDivElement | null>; controls: string; onResize: (n: number) => void;
}) {
  const grab = useRef<{ id: number; at: number; original: number; span: number } | null>(null);
  const vertical = area === "right";
  const { min, max } = splitSizes[area];
  return <div className="dock-splitter" data-area={area} tabIndex={0} role="separator"
    aria-label={`Resize ${dockName[area].toLowerCase()}`} aria-orientation={vertical ? "vertical" : "horizontal"}
    aria-controls={controls} aria-valuemin={min} aria-valuemax={max} aria-valuenow={value} aria-valuetext={`${value}%`}
    title={vertical ? "Resize right dock (Left/Right arrows)" : "Resize bottom dock (Up/Down arrows)"}
    onKeyDown={(e) => {
      const grow = vertical ? "ArrowLeft" : "ArrowUp";
      const shrink = vertical ? "ArrowRight" : "ArrowDown";
      const next = e.key === grow ? value + 3 : e.key === shrink ? value - 3 : e.key === "Home" ? min : e.key === "End" ? max : null;
      if (next === null) return;
      e.preventDefault(); onResize(next);
    }}
    onPointerDown={(e) => {
      if (e.button !== 0 || grab.current) return;
      const rect = container.current?.getBoundingClientRect();
      const span = rect ? (vertical ? rect.width : rect.height) : 0;
      if (!span) return;
      e.preventDefault();
      e.currentTarget.focus();
      e.currentTarget.setPointerCapture(e.pointerId);
      grab.current = { id: e.pointerId, at: vertical ? e.clientX : e.clientY, original: value, span };
    }}
    onPointerMove={(e) => {
      const drag = grab.current;
      if (!drag || e.pointerId !== drag.id) return;
      const delta = drag.at - (vertical ? e.clientX : e.clientY);
      onResize(drag.original + delta * 100 / drag.span);
    }}
    onLostPointerCapture={() => { grab.current = null; }}><i aria-hidden="true"/></div>;
}

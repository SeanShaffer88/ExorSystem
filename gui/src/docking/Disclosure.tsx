import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Icon, type IconName } from "../components/Icon";

/** Ordinary buttons in a disclosure; both the toggle and every action are keyboard reachable. */
export function Disclosure({ label, icon, text, children }: {
  label: string; icon: IconName; text?: string; children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const id = useId();
  const close = () => setOpen(false);
  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => { if (!container.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  return <div ref={container} className="dock-menu" data-open={open}
    onKeyDown={(e) => {
      if (e.key === "Escape" && open) {
        e.preventDefault(); e.stopPropagation(); close(); toggle.current?.focus();
      }
    }}
    onBlur={(e) => { if (!e.currentTarget.contains(e.relatedTarget)) close(); }}>
    <button ref={toggle} type="button" className="dock-control" aria-label={label} title={label}
      aria-expanded={open} aria-controls={id} onClick={() => setOpen((expanded) => !expanded)}>
      <Icon name={icon} size={16}/>{text && <span>{text}</span>}
    </button>
    <div id={id} className="dock-menu-body" hidden={!open}>{children(close)}</div>
  </div>;
}

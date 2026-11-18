import type { ReactNode } from "react";

const shapes = {
  overview: <><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></>,
  apps: <><path d="M3 7h18v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8 7V4h8v3M3 12h18M10 12v2h4v-2"/></>,
  registry: <><rect x="3" y="3" width="6" height="6" rx="1"/><rect x="15" y="3" width="6" height="6" rx="1"/><rect x="15" y="15" width="6" height="6" rx="1"/><path d="M6 9v9h9M9 6h6"/></>,
  changes: <><path d="M9 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h3m6-18h3a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-3M12 3v18M7 9h2m-1-1v2m7 5h2"/></>,
  shield: <><path d="M12 22s7-3 7-10V5l-7-3-7 3v7c0 7 7 10 7 10z"/><path d="m9 12 2 2 4-4"/></>,
  sliders: <><path d="M4 7h16M4 17h16"/><circle cx="8" cy="7" r="2.5" fill="var(--panel)"/><circle cx="16" cy="17" r="2.5" fill="var(--panel)"/></>,
  folder: <path d="M3 6a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>,
  graph: <><circle cx="5" cy="12" r="2.5"/><circle cx="18" cy="5" r="2.5"/><circle cx="18" cy="19" r="2.5"/><path d="m7.3 10.8 8.5-4.6m-8.5 7 8.5 4.6"/></>,
  terminal: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3m6 0h4"/></>,
  pulse: <path d="M2 12h5l3-7 4 14 3-7h5"/>,
  settings: <><path d="m10 3-.6 3-2 .9-2.7-1.4-2 3.5 2.2 2v2l-2.2 2 2 3.5 2.7-1.4 2 .9.6 3h4l.6-3 2-.9 2.7 1.4 2-3.5-2.2-2v-2l2.2-2-2-3.5-2.7 1.4-2-.9L14 3z"/><circle cx="12" cy="12" r="3"/></>,
  palette: <><path d="M12 3a9 9 0 1 0 0 18h1c1.6 0 2.3-1.6 1.1-2.7-.9-.9-.3-2.3 1-2.3H18c2 0 3-1.9 3-4a9 9 0 0 0-9-9z"/><path d="M7 11h.1m3-4h.1m4 0h.1m3 3h.1" strokeWidth="2.7"/></>,
  refresh: <><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.2 7a7 7 0 0 1 11-1.1L20 9M4 15l2.8 3.1A7 7 0 0 0 18 17"/></>,
  plus: <path d="M12 5v14M5 12h14"/>,
  arrow: <path d="M4 12h15m-6-6 6 6-6 6"/>,
  back: <path d="M20 12H5m6-6-6 6 6 6"/>,
  chevron: <path d="m9 6 6 6-6 6"/>,
  clock: <><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
  drive: <><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M3 14h18M7 17h.01M11 17h.01"/></>,
  archive: <><path d="M5 8v11a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8M10 12h4"/><rect x="3" y="3" width="18" height="5" rx="1"/></>,
  file: <><path d="M6 3h8l5 5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM14 3v6h5M9 13h6m-6 4h4"/></>,
  search: <><circle cx="10.5" cy="10.5" r="6"/><path d="m15 15 5 5"/></>,
  check: <path d="m5 12 4 4L19 6"/>,
  close: <path d="m6 6 12 12M6 18 18 6"/>,
  alert: <><path d="M10.4 4.9a1.8 1.8 0 0 1 3.2 0l7.1 12.5A1.8 1.8 0 0 1 19.1 20H4.9a1.8 1.8 0 0 1-1.6-2.6zM12 9v4m0 3h.01"/></>,
  info: <><circle cx="12" cy="12" r="9"/><path d="M12 11v5m0-8h.01"/></>,
  lock: <><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3m-4 4v3"/></>,
  spark: <path d="m12 2 2.6 7.4L22 12l-7.4 2.6L12 22l-2.6-7.4L2 12l7.4-2.6z"/>,
  box: <><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9m-4-15.7 8 4.5"/></>,
  code: <path d="m8 6-6 6 6 6m8-12 6 6-6 6m-2-15-4 18"/>,
  chart: <><path d="M4 4v16h16M8 16v-3m4 3V9m4 7V6"/></>,
  trash: <><path d="M3 6h18M8 6V3h8v3M5 6l1 15h12l1-15M10 10v7m4-7v7"/></>,
  list: <><path d="M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01"/></>,
  monitor: <><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M12 17v4m-4 0h8"/></>,
  layout: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 15h18M15 4v11"/></>,
  dockRight: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M14 4v16m3-11v6"/></>,
  dockBottom: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 13h18m-12 3.5h6"/></>,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof shapes;

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return <svg aria-hidden="true" focusable="false" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round">{shapes[name]}</svg>;
}

export function Logo() {
  return <svg aria-hidden="true" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 2.6 21 7.7v8.6l-9 5.1-9-5.1V7.7L12 2.6Z"/><path d="m7 9 5-2.8L17 9l-5 2.8L7 9Zm0 4 5 2.8 5-2.8M12 15.8v2"/>
  </svg>;
}

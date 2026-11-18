import { useLayoutEffect, useRef, useState, type CSSProperties, type DragEvent, type ReactNode } from "react";
import { Icon } from "../components/Icon";
import { Disclosure } from "./Disclosure";
import { Splitter } from "./Splitter";
import { dockAreas, dockName, locationOf, type DockArea, type SplitArea, type WorkspaceState } from "./layout";
import { panel, tools, type PanelId } from "./panels";

const dragType = "application/x-exorsystem-panel";
export const tabId = (id: PanelId) => `tab-${id}`;
const bodyId = (id: PanelId) => `panel-${id}`;

export function DockWorkspace({ state, compact, onCompact, renderPanel, onOpen, onMove, onClose, onFocus, onResize }: {
  state: WorkspaceState;
  compact: boolean;
  onCompact: (compact: boolean) => void;
  renderPanel: (id: PanelId, visible: boolean) => ReactNode;
  onOpen: (id: PanelId) => void;
  onMove: (id: PanelId, area: DockArea, before?: PanelId) => void;
  onClose: (id: PanelId) => void;
  onFocus: (area: DockArea) => void;
  onResize: (area: SplitArea, value: number) => void;
}) {
  const { layout, initialized } = state;
  const container = useRef<HTMLDivElement>(null);
  const drag = useRef<PanelId | null>(null);
  const [dragging, setDragging] = useState(false);
  const [over, setOver] = useState<DockArea | null>(null);

  useLayoutEffect(() => {
    const el = container.current;
    if (!el) return;
    const measure = () => onCompact(el.clientWidth < 760 || el.clientHeight < 350);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [onCompact]);

  useLayoutEffect(() => {
    for (const area of dockAreas) {
      const id = layout.groups[area].active;
      if (!id || (compact && layout.focus !== area)) continue;
      const tab = document.getElementById(tabId(id));
      const strip = tab?.parentElement;
      if (!tab || !strip) continue;
      const item = tab.getBoundingClientRect(), track = strip.getBoundingClientRect();
      // Scroll only the tab strip. scrollIntoView can move the window's sequential Tab origin.
      if (item.left < track.left) strip.scrollLeft -= track.left - item.left + 2;
      else if (item.right > track.right) strip.scrollLeft += item.right - track.right + 2;
    }
  }, [layout.groups, layout.focus, compact]);

  const startDrag = (e: DragEvent, id: PanelId) => {
    drag.current = id;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData(dragType, id);
    setDragging(true);
  };
  const stopDrag = () => { drag.current = null; setDragging(false); setOver(null); };
  const accept = (e: DragEvent) => !!drag.current && e.dataTransfer.types.includes(dragType);
  const dropEvents = (area: DockArea, before?: PanelId) => ({
    onDragOver: (e: DragEvent<HTMLElement>) => {
      if (!accept(e)) return;
      e.preventDefault(); e.stopPropagation();
      e.dataTransfer.dropEffect = "move"; setOver(area);
    },
    onDrop: (e: DragEvent<HTMLElement>) => {
      if (!accept(e)) return;
      e.preventDefault(); e.stopPropagation();
      const id = drag.current;
      const matches = e.dataTransfer.getData(dragType) === id;
      stopDrag();
      if (id && matches) onMove(id, area, before);
    },
  });

  const shown = (area: DockArea) => !compact || layout.focus === area;
  const otherDocks = dockAreas.filter((a) => a !== "main" && layout.groups[a].active);
  const frame = (id: PanelId, area?: DockArea) => {
    const visible = !!area && shown(area) && layout.groups[area].active === id;
    return <div key={`panel-${id}`} id={bodyId(id)} className="dock-panel" data-area={area} data-panel={id}
      role="tabpanel" aria-labelledby={tabId(id)} tabIndex={0} hidden={!visible}
      onFocusCapture={() => { if (area) onFocus(area); }} {...(area ? dropEvents(area) : {})}>
      {initialized.has(id) && renderPanel(id, visible)}
    </div>;
  };

  // All panel frames are keyed siblings, even when inactive or closed. Moving one does not
  // remount it: form input, trace selections and in-progress Watch/Scan work survive docking.
  const items: ReactNode[] = [];
  if (compact && otherDocks.length > 0) {
    items.push(<nav key="compact" className="dock-switcher" aria-label="Compact docks">
      {(["main", ...otherDocks] as DockArea[]).map((area) => <button key={area} type="button" aria-pressed={area === layout.focus}
        onClick={() => { const id = layout.groups[area].active; if (id) onOpen(id); else onFocus(area); }}>
        <Icon name={area === "main" ? "monitor" : area === "right" ? "dockRight" : "dockBottom"} size={14}/>{dockName[area]}
      </button>)}
    </nav>);
  }

  for (const area of dockAreas) {
    const { ids, active } = layout.groups[area];
    if (area !== "main" && !active) continue;
    if (area !== "main" && !compact && active) {
      items.push(<Splitter key={`split-${area}`} area={area} value={layout.sizes[area]} container={container}
        controls={bodyId(active)} onResize={(value) => onResize(area, value)}/>);
    }
    items.push(<div key={`bar-${area}`} className="dock-bar" data-area={area} data-current={layout.focus === area} data-over={dragging && over === area}
      hidden={!shown(area)} onFocusCapture={() => onFocus(area)} {...dropEvents(area)}>
      <span className="dock-position" title={dockName[area]} aria-hidden="true">
        <Icon name={area === "main" ? "monitor" : area === "right" ? "dockRight" : "dockBottom"} size={15}/>
      </span>
      {ids.length ? <div className="dock-tabs" role="tablist" aria-label={`${dockName[area]} panels`}>
        {ids.map((id, index) => <button key={id} id={tabId(id)} role="tab" type="button" className="dock-tab"
          aria-selected={id === active} aria-controls={bodyId(id)} tabIndex={id === active ? 0 : -1}
          title={`${panel(id).title} · drag to dock`} draggable onDragStart={(e) => startDrag(e, id)} onDragEnd={stopDrag}
          onClick={() => onOpen(id)} {...dropEvents(area, id)}
          onKeyDown={(e) => {
            const arrow = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
            if (e.key === "Delete") { e.preventDefault(); if (!e.repeat) onClose(id); return; }
            if (arrow && e.ctrlKey) {
              e.preventDefault();
              if (index + arrow >= 0 && index + arrow < ids.length) onMove(id, area, ids[index + (arrow < 0 ? -1 : 2)]);
              return;
            }
            const next = arrow ? (index + arrow + ids.length) % ids.length : e.key === "Home" ? 0 : e.key === "End" ? ids.length - 1 : -1;
            const target = ids[next];
            if (target) { e.preventDefault(); onOpen(target); }
          }}>
          <Icon name={panel(id).icon} size={15}/><span>{panel(id).title}</span>
        </button>)}
      </div> : <span className="dock-empty-label">Main workspace</span>}
      {active && <div className="dock-actions">
        <Disclosure key={active} label={`Dock ${panel(active).title}`} icon="layout">{(close) => <>
          <strong>Move panel to</strong>
          {dockAreas.filter((a) => a !== area).map((destination) => <button key={destination} type="button" onClick={() => { close(); onMove(active, destination); }}>
            <Icon name={destination === "main" ? "monitor" : destination === "right" ? "dockRight" : "dockBottom"} size={15}/>{dockName[destination]}
          </button>)}
          <small className="muted">You can also drag its tab.</small>
        </>}</Disclosure>
        <button type="button" className="dock-control" aria-label={`Close ${panel(active).title}`} title={`Close ${panel(active).title}`}
          onClick={() => onClose(active)}><Icon name="close" size={15}/></button>
      </div>}
    </div>);
    for (const id of ids) items.push(frame(id, area));
    if (!active) items.push(<div key="empty-main" className="dock-panel dock-empty" data-area="main" hidden={!shown("main")} {...dropEvents("main")}>
      <span className="tile-icon tone-safe"><Icon name="layout"/></span><h1>Main dock is empty</h1>
      <p className="muted">Open a tool from the sidebar or drop a panel tab here.</p>
      <button type="button" className="quiet-button" onClick={() => onMove("overview", "main")}>Open Overview</button>
    </div>);
  }
  for (const id of tools) if (initialized.has(id) && !locationOf(layout, id)) items.push(frame(id));

  if (dragging) {
    const targets = compact ? dockAreas : dockAreas.filter((area) => area !== "main" && !layout.groups[area].active);
    for (const area of targets) items.push(<div key={`drop-${area}`} className="dock-drop-target" data-area={area}
      data-over={over === area} aria-hidden="true" {...dropEvents(area)}><Icon name={area === "main" ? "monitor" : area === "right" ? "dockRight" : "dockBottom"}/><strong>{dockName[area]}</strong></div>);
  }

  const sizes = { "--right-size": `${layout.sizes.right}%`, "--bottom-size": `${layout.sizes.bottom}%` } as CSSProperties;
  return <div ref={container} className="dock-space" data-compact={compact} data-right={!!layout.groups.right.active}
    data-bottom={!!layout.groups.bottom.active} data-dragging={dragging} style={sizes}
    onDragLeave={(e) => { if (!(e.relatedTarget instanceof Node) || !e.currentTarget.contains(e.relatedTarget)) setOver(null); }}>
    {items}
  </div>;
}

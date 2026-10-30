import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** A grid of square cells that mounts only the rows on screen, so a chat's
 *  hundreds of photos do not all load at once. Give `columns` for a fixed
 *  count, or `minCell` to fit as many as the width allows. */
export function VirtualGrid({
  count,
  columns,
  minCell = 120,
  gap = 2,
  padding = 2,
  className = "",
  scrollTo,
  render,
}: {
  count: number;
  columns?: number;
  minCell?: number;
  gap?: number;
  padding?: number;
  className?: string;
  /** Bring this cell into view on mount and when it changes. */
  scrollTo?: number;
  render: (index: number) => React.ReactNode;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const inner = Math.max(0, width - padding * 2);
  const cols = columns ?? Math.max(1, Math.floor((inner + gap) / (minCell + gap)));
  const cell = cols ? (inner - gap * (cols - 1)) / cols : 0;
  const rows = Math.ceil(count / cols);
  const virtual = useVirtualizer({
    count: rows,
    getScrollElement: () => scroller.current,
    estimateSize: () => cell + gap,
    overscan: 6,
    paddingStart: padding,
    paddingEnd: padding,
  });
  useEffect(() => virtual.measure(), [cell, virtual]);
  useEffect(() => {
    if (scrollTo != null && scrollTo >= 0 && cell > 0) virtual.scrollToIndex(Math.floor(scrollTo / cols), { align: "center" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollTo, cols, cell > 0]);
  return (
    <div ref={scroller} className={`min-h-0 overflow-y-auto ${className}`}>
      <div className="relative w-full" style={{ height: virtual.getTotalSize() }}>
        {cell > 0 &&
          virtual.getVirtualItems().map((row) => (
            <div
              key={row.key}
              className="absolute flex"
              style={{ top: row.start, left: padding, right: padding, height: cell, gap }}
            >
              {Array.from({ length: Math.min(cols, count - row.index * cols) }, (_, c) => {
                const i = row.index * cols + c;
                return (
                  <div key={i} className="relative shrink-0" style={{ width: cell, height: cell }}>
                    {render(i)}
                  </div>
                );
              })}
            </div>
          ))}
      </div>
    </div>
  );
}

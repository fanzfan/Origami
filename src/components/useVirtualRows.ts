import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

interface Layout {
  viewportHeight: number;
  headerHeight: number;
  fixedRowHeight: number;
  rowHeight: number;
}

interface VirtualRows {
  containerRef: React.RefObject<HTMLDivElement>;
  headerRef: React.RefObject<HTMLTableSectionElement>;
  fixedRowRef: React.RefObject<HTMLTableRowElement>;
  startIndex: number;
  endIndex: number;
  topPadding: number;
  bottomPadding: number;
}

const OVERSCAN_ROWS = 8;
const DEFAULT_ROW_HEIGHT = 36;
const DEFAULT_VIEWPORT_HEIGHT = 640;

// getBoundingClientRect 会包含元素及所有祖先的 CSS zoom；clientHeight、scrollTop 不会。
// 用浮点布局尺寸除以祖先 zoom 乘积，避免把 100k 行的整数舍入误差累计进 spacer。
function unzoomedHeight(element: Element): number {
  let zoom = 1;
  let ancestor: Element | null = element;
  while (ancestor) {
    const value = Number.parseFloat(window.getComputedStyle(ancestor).zoom);
    if (Number.isFinite(value) && value > 0) zoom *= value;
    ancestor = ancestor.parentElement;
  }
  return element.getBoundingClientRect().height / zoom;
}

/** 只限制挂载的行；完整数据仍由调用方用于排序、搜索和选择。 */
export function useVirtualRows(count: number, resetKey: string, hasFixedRow: boolean): VirtualRows {
  const containerRef = useRef<HTMLDivElement>(null!);
  const headerRef = useRef<HTMLTableSectionElement>(null!);
  const fixedRowRef = useRef<HTMLTableRowElement>(null!);
  const [scrollTop, setScrollTop] = useState(0);
  const [layout, setLayout] = useState<Layout>({
    viewportHeight: DEFAULT_VIEWPORT_HEIGHT,
    headerHeight: 36,
    fixedRowHeight: hasFixedRow ? DEFAULT_ROW_HEIGHT : 0,
    rowHeight: DEFAULT_ROW_HEIGHT,
  });
  const resetKeyRef = useRef(resetKey);

  const measure = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    const firstRow = container.querySelector<HTMLTableRowElement>("tbody tr.virtual-row[data-row-index]");
    const next: Layout = {
      viewportHeight: container.clientHeight,
      headerHeight: headerRef.current ? unzoomedHeight(headerRef.current) : 0,
      fixedRowHeight: hasFixedRow && fixedRowRef.current ? unzoomedHeight(fixedRowRef.current) : 0,
      rowHeight: firstRow ? unzoomedHeight(firstRow) : layout.rowHeight,
    };
    setLayout((current) =>
      current.viewportHeight === next.viewportHeight &&
      Math.abs(current.headerHeight - next.headerHeight) < 0.0001 &&
      Math.abs(current.fixedRowHeight - next.fixedRowHeight) < 0.0001 &&
      Math.abs(current.rowHeight - next.rowHeight) < 0.0001
        ? current
        : next,
    );
  }, [hasFixedRow, layout.rowHeight]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(container);
    if (headerRef.current) observer?.observe(headerRef.current);
    if (fixedRowRef.current) observer?.observe(fixedRowRef.current);
    const sample = container.querySelector<HTMLTableRowElement>("tbody tr.virtual-row[data-row-index]");
    if (sample) observer?.observe(sample);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [count, hasFixedRow, measure]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let frame = 0;
    const onScroll = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        setScrollTop(container.scrollTop);
      });
    };
    container.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      container.removeEventListener("scroll", onScroll);
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, []);

  useLayoutEffect(() => {
    if (resetKeyRef.current === resetKey) return;
    resetKeyRef.current = resetKey;
    if (containerRef.current) containerRef.current.scrollTop = 0;
    setScrollTop(0);
  }, [resetKey]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const maxScrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
    if (container.scrollTop > maxScrollTop) container.scrollTop = maxScrollTop;
    setScrollTop(container.scrollTop);
  }, [count, hasFixedRow]);

  const range = useMemo(() => {
    const rowHeight = layout.rowHeight > 0 ? layout.rowHeight : DEFAULT_ROW_HEIGHT;
    // sticky 表头遮住视口顶部的 headerHeight 像素；父目录行占固定偏移。
    // 可见数据区换算成滚动内容坐标后，首尾索引分别为
    // floor((scrollTop - fixedRowHeight) / rowHeight) 与
    // ceil((scrollTop + viewportHeight - headerHeight - fixedRowHeight) / rowHeight)。
    const startIndex = Math.min(
      count,
      Math.max(0, Math.floor((scrollTop - layout.fixedRowHeight) / rowHeight) - OVERSCAN_ROWS),
    );
    const endIndex = Math.min(
      count,
      Math.max(
        startIndex,
        Math.ceil((scrollTop + layout.viewportHeight - layout.headerHeight - layout.fixedRowHeight) / rowHeight) +
          OVERSCAN_ROWS,
      ),
    );
    return {
      startIndex,
      endIndex,
      topPadding: startIndex * rowHeight,
      bottomPadding: Math.max(0, count - endIndex) * rowHeight,
    };
  }, [count, layout, scrollTop]);

  return { containerRef, headerRef, fixedRowRef, ...range };
}

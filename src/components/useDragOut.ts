import { useCallback, useEffect, useRef } from "react";

/** 越过多少像素才算「开始拖动」，与系统文件管理器的手感接近。 */
const THRESHOLD = 6;

/**
 * 「按住某一行并移开」的拖出手势。
 *
 * 不走 HTML5 的 dragstart：WKWebView 对它的支持不可靠（密码列表的排序拖动同样绕开了
 * 它，见 dialogs.tsx）。而且鼠标一越过阈值就能拿到回调，比 dragstart 更早，正好用来
 * 抢先解压——系统原生拖放要求发起的一刻左键仍按着，准备得越早越不容易错过。
 *
 * @param onBegin 越过阈值时调用，入参是按下那一行的标识（包内路径 / 真实路径）。
 * @returns 挂到行 onMouseDown 上的处理器。
 */
export function useDragOut(onBegin: (key: string) => void) {
  const pending = useRef<{ key: string; x: number; y: number } | null>(null);
  // 用 ref 转发，回调里才能读到当前这一次渲染的选中集合。
  const begin = useRef(onBegin);
  begin.current = onBegin;

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const p = pending.current;
      if (!p) return;
      if (Math.abs(e.clientX - p.x) < THRESHOLD && Math.abs(e.clientY - p.y) < THRESHOLD) return;
      pending.current = null; // 一次按下只触发一次
      begin.current(p.key);
    };
    const onUp = () => {
      pending.current = null;
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  // 只认鼠标左键；带修饰键的按下是多选/连选，不该变成拖动。
  return useCallback((key: string, e: React.MouseEvent) => {
    if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey) {
      pending.current = null;
      return;
    }
    pending.current = { key, x: e.clientX, y: e.clientY };
  }, []);
}

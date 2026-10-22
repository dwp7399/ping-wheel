import { useCallback, useMemo, useState } from "react";
import Wheel, { WHEEL_SIZE, WheelSource } from "./Wheel";
import { ICONS, MAX_ITEMS, WheelItem, selectionFor, suggestAngle } from "./types";

const DEMO_LABELS = ["问号", "标记", "收藏", "对话", "左上", "右上", "右下", "左下", "翻译", "笔记", "终端", "代码"];
const DEMO_ICONS = ["ask", "mark", "collect", "chat", "sparkle", "translate", "note", "camera", "terminal", "folder", "code", "link"] as const;

/** 按「先四正、再四斜、再补空隙」的顺序生成 n 个演示项 */
export function demoItems(n: number): WheelItem[] {
  const items: WheelItem[] = [];
  for (let i = 0; i < n; i++) {
    const angle = suggestAngle(items.map((it) => it.angle));
    if (angle === null) break;
    items.push({
      id: `demo-${i}`,
      angle,
      label: DEMO_LABELS[i],
      icon: DEMO_ICONS[i] ?? ICONS[i % ICONS.length],
      action: i === 0 ? { type: "ask", prompt: "" } : { type: "chat" },
    });
  }
  return items;
}

/**
 * 浏览器预览：鼠标在舞台内移动即高亮，点击显示将执行的项。
 * URL 参数：?n=8 数量，?dx=..&dy=.. 固定位移，便于截图检查。
 */
export default function Preview() {
  const params = new URLSearchParams(location.search);
  const [n, setN] = useState(() => Math.min(MAX_ITEMS, Math.max(1, Number(params.get("n") ?? 4))));
  const [bg, setBg] = useState<"doc" | "dark" | "photo">("doc");
  const [last, setLast] = useState("");
  const items = useMemo(() => demoItems(n), [n]);
  const angles = items.map((it) => it.angle);

  const source: WheelSource = useCallback(
    (h) => {
      h.open();
      if (params.has("dx")) {
        const dx = Number(params.get("dx"));
        const dy = Number(params.get("dy") ?? 0);
        setTimeout(() => h.move(dx, dy, selectionFor(dx, dy, angles)), 150);
      }
      const stage = document.getElementById("stage");
      const delta = (e: MouseEvent) => {
        const r = stage!.getBoundingClientRect();
        return [e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2)] as const;
      };
      const onMove = (e: MouseEvent) => {
        const [dx, dy] = delta(e);
        h.move(dx, dy, selectionFor(dx, dy, angles));
      };
      const onDown = (e: MouseEvent) => {
        const [dx, dy] = delta(e);
        const sel = selectionFor(dx, dy, angles);
        setLast(sel === null ? "菜单" : items[sel].label);
      };
      stage?.addEventListener("mousemove", onMove);
      stage?.addEventListener("mousedown", onDown);
      return () => {
        stage?.removeEventListener("mousemove", onMove);
        stage?.removeEventListener("mousedown", onDown);
      };
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [items],
  );

  return (
    <div className="preview">
      <div className="preview-bar">
        <span>轮盘预览（⌘⇧C）：鼠标指向高亮，点击执行{last && ` · 刚才点了「${last}」`}</span>
        <label>
          数量 {n}
          <input type="range" min={1} max={MAX_ITEMS} value={n} onChange={(e) => setN(Number(e.target.value))} />
        </label>
        {(["doc", "dark", "photo"] as const).map((b) => (
          <button key={b} className={bg === b ? "on" : ""} onClick={() => setBg(b)}>
            {{ doc: "浅色文档", dark: "深色界面", photo: "花哨背景" }[b]}
          </button>
        ))}
      </div>
      <div id="stage" className={`preview-stage bg-${bg}`}>
        <div style={{ width: WHEEL_SIZE, height: WHEEL_SIZE }}>
          <Wheel source={source} items={items} />
        </div>
      </div>
    </div>
  );
}

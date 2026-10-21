import { useEffect, useMemo, useRef } from "react";
import { Icon } from "./icons";
import { DEAD_ZONE, Selection, WheelItem, actionHint, sectorOf } from "./types";

/** 轮盘窗口边长，需与 tauri.conf.json / wheel.rs 的 WHEEL_SIZE 一致 */
export const WHEEL_SIZE = 440;
const C = WHEEL_SIZE / 2;
/** 方向指示环半径 */
const R_RING = 36;
const ARC_SPAN = 56;

export type WheelHandlers = {
  open: () => void;
  move: (dx: number, dy: number, index: Selection) => void;
};

/** 事件来源：Tauri 下来自 Rust，浏览器预览下来自鼠标模拟 */
export type WheelSource = (h: WheelHandlers) => () => void;

/** 相邻两项的最小夹角 */
function minGap(angles: number[]) {
  if (angles.length < 2) return 360;
  const s = [...angles].sort((a, b) => a - b);
  return Math.min(...s.map((a, i) => (i === s.length - 1 ? s[0] + 360 - a : s[i + 1] - a)));
}

/** 按最密处的夹角决定卡片尺寸和半径，保证相邻卡片不重叠 */
export function layoutFor(angles: number[]) {
  const gap = minGap(angles);
  if (gap >= 90) return { r: 98, w: 72, h: 62, font: 12, icon: 22 };
  if (gap >= 45) return { r: 114, w: 66, h: 56, font: 12, icon: 20 };
  return { r: 150, w: 54, h: 48, font: 10.5, icon: 17 };
}

export function polar(r: number, deg: number): [number, number] {
  // deg：从正上方顺时针
  const a = (deg * Math.PI) / 180;
  return [C + r * Math.sin(a), C - r * Math.cos(a)];
}

/** 扇区填充：从 a0 到 a1（顺时针）的环形扇区 */
function sectorPath(a0: number, a1: number, r0: number, r1: number) {
  const span = a1 - a0;
  if (span >= 360) {
    return `M${C},${C - r1} A${r1},${r1} 0 1 1 ${C - 0.01},${C - r1} Z M${C},${C - r0} A${r0},${r0} 0 1 0 ${C + 0.01},${C - r0} Z`;
  }
  const [x0, y0] = polar(r1, a0);
  const [x1, y1] = polar(r1, a1);
  const [x2, y2] = polar(r0, a1);
  const [x3, y3] = polar(r0, a0);
  const large = span > 180 ? 1 : 0;
  return `M${x0},${y0} A${r1},${r1} 0 ${large} 1 ${x1},${y1} L${x2},${y2} A${r0},${r0} 0 ${large} 0 ${x3},${y3} Z`;
}

function arcPath(r: number, span: number) {
  const a = (span / 2) * (Math.PI / 180);
  return `M${C + r * Math.cos(-a)},${C + r * Math.sin(-a)} A${r},${r} 0 0 1 ${C + r * Math.cos(a)},${C + r * Math.sin(a)}`;
}

const key = (s: Selection) => (s === null ? "c" : String(s));

export default function Wheel({ source, items }: { source: WheelSource; items: WheelItem[] }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const arcRef = useRef<SVGGElement>(null);
  const hintRef = useRef<HTMLDivElement>(null);

  const angles = useMemo(() => items.map((it) => it.angle), [items]);
  const L = layoutFor(angles);
  const hintTop = C + L.r + L.h / 2 + 20;

  const hints = useMemo(() => items.map((it) => actionHint(it.action)), [items]);

  useEffect(() => {
    let current: Selection = null;

    const setHint = () => {
      const el = hintRef.current;
      if (!el) return;
      const action = current === null ? "打开菜单和设置" : hints[current] ?? "";
      el.querySelector("b")!.textContent = action;
      el.querySelector("span")!.textContent = "点击执行 · 右键或 Esc 取消";
    };

    const select = (sel: Selection) => {
      const root = rootRef.current;
      if (!root) return;
      root.querySelectorAll(".active").forEach((el) => el.classList.remove("active"));
      root.querySelectorAll(`[data-key="${key(sel)}"]`).forEach((el) => el.classList.add("active"));
      current = sel;
      setHint();
    };

    // 高频事件直接改 DOM，不走 React 渲染，保证跟手
    const move = (dx: number, dy: number, sel: Selection) => {
      if (sel !== current) select(sel);
      const arc = arcRef.current;
      if (!arc) return;
      const dist = Math.hypot(dx, dy);
      const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
      arc.style.transform = `rotate(${angle}deg)`;
      arc.style.opacity = String(Math.min(1, Math.max(0, (dist - 4) / DEAD_ZONE)));
    };

    select(null);
    const unsubscribe = source({
      open: () => {
        select(null);
        move(0, 0, null);
        const root = rootRef.current;
        if (root) {
          root.classList.remove("pop");
          void root.offsetWidth;
          root.classList.add("pop");
        }
      },
      move,
    });
    return unsubscribe;
  }, [source, hints]);

  return (
    <div
      ref={rootRef}
      className="wheel pop"
      style={{ width: WHEEL_SIZE, height: WHEEL_SIZE, ["--chip-font" as string]: `${L.font}px` }}
    >
      <div className="wheel-halo" style={{ inset: C - L.r - L.h / 2 - 16 }} />

      <svg className="wheel-ring" width={WHEEL_SIZE} height={WHEEL_SIZE}>
        {items.map((it, i) => {
          const [a0, a1] = sectorOf(angles, i);
          return (
            <path
              key={it.id}
              data-key={i}
              className="sector"
              d={sectorPath(a0, a1, R_RING + 8, L.r + L.h / 2 + 8)}
            />
          );
        })}
        <circle cx={C} cy={C} r={R_RING} className="ring-track" />
        <g ref={arcRef} className="ring-arc" style={{ transformOrigin: `${C}px ${C}px`, opacity: 0 }}>
          <path d={arcPath(R_RING, ARC_SPAN)} />
        </g>
      </svg>

      <div data-key="c" className="hub" style={{ left: C, top: C }}>
        <Icon name="menu" size={16} />
      </div>

      {items.map((it, i) => {
        const [x, y] = polar(L.r, it.angle);
        const rad = (it.angle * Math.PI) / 180;
        const [nx, ny] = [Math.sin(rad), -Math.cos(rad)];
        return (
          <div
            key={it.id}
            data-key={i}
            className="chip"
            style={{
              left: x,
              top: y,
              width: L.w,
              height: L.h,
              ["--nx" as string]: nx,
              ["--ny" as string]: ny,
            }}
          >
            <Icon name={it.icon} size={L.icon} />
            <span>{it.label}</span>
          </div>
        );
      })}

      <div ref={hintRef} className="wheel-hint" style={{ top: Math.min(hintTop, WHEEL_SIZE - 34) }}>
        <b />
        <span />
      </div>
    </div>
  );
}

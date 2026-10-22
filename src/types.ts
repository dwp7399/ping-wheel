import { t } from "./i18n";

export type Action =
  | { type: "ask"; prompt: string; mode: string; custom: string }
  | { type: "mark" }
  | { type: "collect" }
  | { type: "chat" }
  | { type: "open"; target: string }
  | { type: "plugin"; plugin: string; action: string };

export type ActionType = Action["type"];

export type WheelItem = {
  id: string;
  /** 方位：从正上方顺时针的角度，22.5 的整数倍 */
  angle: number;
  label: string;
  icon: IconName;
  action: Action;
};

export type WheelConfig = { items: WheelItem[] };

/** 当前选择：null 为中心菜单，数字为方向项下标 */
export type Selection = number | null;

export const MAX_ITEMS = 12;

/** 中心死区半径（逻辑像素），需与 src-tauri/src/wheel.rs 的 DEAD_ZONE 一致 */
export const DEAD_ZONE = 30;

export const ICONS = [
  "ask",
  "mark",
  "collect",
  "chat",
  "link",
  "translate",
  "note",
  "camera",
  "terminal",
  "sparkle",
  "folder",
  "code",
] as const;
export type IconName = (typeof ICONS)[number] | "menu";

export const ACTION_TYPES: Record<ActionType, { label: string; hint: string; icon: IconName }> = {
  ask: { label: "?", hint: t("理解标记处的内容"), icon: "ask" },
  mark: { label: t("标记"), hint: t("记下当前场景，之后一键回来"), icon: "mark" },
  collect: { label: t("收藏"), hint: t("截图收藏，自动归类"), icon: "collect" },
  chat: { label: t("对话"), hint: t("新开一个对话"), icon: "chat" },
  open: { label: t("打开"), hint: t("打开链接、文件或应用"), icon: "link" },
  plugin: { label: t("插件"), hint: t("运行插件"), icon: "code" },
};

export function actionHint(a: Action): string {
  if (a.type === "ask") {
    if (a.prompt.trim()) return t("看标记处：{prompt}", { prompt: a.prompt.trim() });
    if (a.mode && a.mode !== "auto") return askModeName(a.mode);
  }
  if (a.type === "open" && a.target.trim()) return t("打开 {target}", { target: a.target.trim() });
  return ACTION_TYPES[a.type].hint;
}

export function newAction(type: ActionType): Action {
  if (type === "ask") return { type, prompt: "", mode: "auto", custom: "" };
  if (type === "open") return { type, target: "" };
  if (type === "plugin") return { type, plugin: "", action: "" };
  return { type };
}

/** 「?」的主要目的（附加提示词在后端，管理台可改） */
export const ASK_MODES: { id: string; name: string }[] = [
  { id: "auto", name: t("自动判断") },
  { id: "translate", name: t("翻译") },
  { id: "explain", name: t("名词解释") },
  { id: "reading", name: t("阅读理解") },
  { id: "summarize", name: t("总结要点") },
  { id: "code", name: t("代码解释") },
  { id: "error", name: t("报错排查") },
  { id: "data", name: t("图表解读") },
  { id: "ui", name: t("界面怎么用") },
  { id: "custom", name: t("自定义") },
];

export const askModeName = (id: string) => ASK_MODES.find((m) => m.id === id)?.name ?? id;

export const ANGLE_STEP = 22.5;

/** 16 个可选方位，从正上方顺时针 */
export const DIRECTIONS: { angle: number; name: string }[] = [
  "上", "上偏右", "右上", "右偏上", "右", "右偏下", "右下", "下偏右",
  "下", "下偏左", "左下", "左偏下", "左", "左偏上", "左上", "上偏左",
].map((name, i) => ({ angle: i * ANGLE_STEP, name: t(name) }));

export const directionName = (angle: number) =>
  DIRECTIONS[Math.round(angle / ANGLE_STEP) % DIRECTIONS.length].name;

/** 两个方位之间的夹角（0～180） */
export function angleDiff(a: number, b: number) {
  const d = (((a - b) % 360) + 360) % 360;
  return Math.min(d, 360 - d);
}

/** 与 Rust 端 selection_for 一致：死区内为中心，否则取方位最近的一项 */
export function selectionFor(dx: number, dy: number, angles: number[]): Selection {
  if (angles.length === 0 || Math.hypot(dx, dy) < DEAD_ZONE) return null;
  const angle = (((Math.atan2(dx, -dy) * 180) / Math.PI) % 360 + 360) % 360;
  let best = 0;
  angles.forEach((a, i) => {
    if (angleDiff(angle, a) < angleDiff(angle, angles[best])) best = i;
  });
  return best;
}

/**
 * 第 i 项的命中扇区 [起, 止]（顺时针，止可能大于 360）：
 * 两侧边界分别是与前后相邻项的角平分线。
 */
export function sectorOf(angles: number[], i: number): [number, number] {
  if (angles.length <= 1) return [0, 360];
  const sorted = [...angles].sort((a, b) => a - b);
  const k = sorted.indexOf(angles[i]);
  const prev = sorted[(k - 1 + sorted.length) % sorted.length];
  const next = sorted[(k + 1) % sorted.length];
  const a = angles[i];
  const gapPrev = (((a - prev) % 360) + 360) % 360;
  const gapNext = (((next - a) % 360) + 360) % 360;
  return [a - gapPrev / 2, a + gapNext / 2];
}

/** 推荐的新增方位：先补四个正方向，再补四个斜方向，最后取最大空隙的中点 */
export function suggestAngle(angles: number[]): number | null {
  const taken = new Set(angles.map((a) => Math.round(a / ANGLE_STEP)));
  for (const a of [270, 0, 90, 180, 315, 45, 135, 225]) {
    if (!taken.has(a / ANGLE_STEP)) return a;
  }
  const free = DIRECTIONS.filter((d) => !taken.has(Math.round(d.angle / ANGLE_STEP)));
  if (free.length === 0) return null;
  // 离已有项最远的空位
  const score = (a: number) => Math.min(...angles.map((b) => angleDiff(a, b)));
  return free.reduce((best, d) => (score(d.angle) > score(best.angle) ? d : best)).angle;
}

export const DEFAULT_CONFIG: WheelConfig = {
  items: [
    { id: "mark", angle: 0, label: t("标记"), icon: "mark", action: { type: "mark" } },
    { id: "collect", angle: 90, label: t("收藏"), icon: "collect", action: { type: "collect" } },
    { id: "chat", angle: 180, label: t("对话"), icon: "chat", action: { type: "chat" } },
    { id: "ask", angle: 270, label: "?", icon: "ask", action: { type: "ask", prompt: "", mode: "auto", custom: "" } },
  ],
};

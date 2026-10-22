import { useEffect, useMemo, useState } from "react";
import Wheel, { WHEEL_SIZE, WheelSource, layoutFor, polar } from "./Wheel";
import { Icon } from "./icons";
import { PluginInfo, api } from "./api";
import {
  ACTION_TYPES,
  ANGLE_STEP,
  ActionType,
  DIRECTIONS,
  ICONS,
  MAX_ITEMS,
  WheelItem,
  directionName,
  newAction,
  suggestAngle,
} from "./types";

/** 静态预览：高亮编辑中的那一项 */
const staticSource =
  (highlight: number | null): WheelSource =>
  (h) => {
    if (highlight !== null) h.move(0, 0, highlight);
    return () => {};
  };

function IconPicker({ value, onChange }: { value: WheelItem["icon"]; onChange: (v: WheelItem["icon"]) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="icon-picker" onBlur={(e) => !e.currentTarget.contains(e.relatedTarget) && setOpen(false)}>
      <button className="icon-btn" title="选择图标" onClick={() => setOpen((o) => !o)}>
        <Icon name={value} size={18} />
      </button>
      {open && (
        <div className="icon-grid">
          {ICONS.map((name) => (
            <button
              key={name}
              className={name === value ? "on" : ""}
              onClick={() => {
                onChange(name);
                setOpen(false);
              }}
            >
              <Icon name={name} size={18} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

const byAngle = (list: WheelItem[]) => [...list].sort((a, b) => a.angle - b.angle);
const slot = (angle: number) => Math.round(angle / ANGLE_STEP);

/** 预览里可点的空位：空着的四正四斜方向，以及相邻两项正中间的空位 */
function ghostAngles(items: WheelItem[]) {
  const taken = new Set(items.map((it) => slot(it.angle)));
  const out = new Set<number>();
  for (let a = 0; a < 360; a += 45) if (!taken.has(slot(a))) out.add(a);
  const sorted = byAngle(items).map((it) => it.angle);
  sorted.forEach((a, i) => {
    const next = i === sorted.length - 1 ? sorted[0] + 360 : sorted[i + 1];
    const mid = ((a + next) / 2) % 360;
    if (Number.isInteger(mid / ANGLE_STEP) && !taken.has(slot(mid))) out.add(mid);
  });
  return [...out].sort((a, b) => a - b);
}

let seq = 0;
const newId = () => `item-${Date.now().toString(36)}-${seq++}`;

export default function WheelEditor({
  initial,
  onSave,
  onReset,
}: {
  initial: WheelItem[];
  onSave: (items: WheelItem[]) => Promise<void>;
  onReset: () => Promise<WheelItem[]>;
}) {
  const [items, setItemsRaw] = useState(() => byAngle(initial));
  const setItems = (u: WheelItem[] | ((l: WheelItem[]) => WheelItem[])) =>
    setItemsRaw((l) => byAngle(typeof u === "function" ? u(l) : u));
  const [pending, setPending] = useState<number | null>(() => suggestAngle(initial.map((it) => it.angle)));
  const [focus, setFocus] = useState<number | null>(null);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const source = useMemo(() => staticSource(focus), [focus]);
  const dirty = useMemo(
    () => JSON.stringify(items) !== JSON.stringify(byAngle(initial)),
    [items, initial],
  );
  const ghosts = useMemo(() => (items.length < MAX_ITEMS ? ghostAngles(items) : []), [items]);
  const L = layoutFor(items.map((it) => it.angle));

  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => setItems(initial), [initial]);

  // 空位被占后，重新推荐一个
  useEffect(() => {
    const taken = new Set(items.map((it) => slot(it.angle)));
    if (pending === null || taken.has(slot(pending))) setPending(suggestAngle(items.map((it) => it.angle)));
  }, [items, pending]);

  const update = (i: number, patch: Partial<WheelItem>) =>
    setItems((list) => list.map((it, j) => (j === i ? { ...it, ...patch } : it)));

  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  useEffect(() => {
    api.plugins().then(setPlugins).catch(() => setPlugins([]));
  }, []);
  // 插件动作：「插件 id::动作 id」→ 插件信息
  const pluginActions = useMemo(
    () =>
      plugins.flatMap((p) =>
        p.manifest.actions.map((a) => ({ key: `${p.manifest.id}::${a.id}`, plugin: p.manifest, action: a })),
      ),
    [plugins],
  );

  const insert = (item: WheelItem) => {
    if (items.length >= MAX_ITEMS || pending === null) return;
    const next = byAngle([...items, { ...item, angle: pending }]);
    setItems(next);
    setFocus(next.findIndex((x) => x.id === item.id));
  };

  const add = (type: ActionType) => {
    const meta = ACTION_TYPES[type];
    insert({ id: newId(), angle: 0, label: meta.label, icon: meta.icon, action: newAction(type) });
  };

  const addPlugin = (key: string) => {
    const pa = pluginActions.find((x) => x.key === key);
    if (!pa) return;
    insert({
      id: newId(),
      angle: 0,
      label: pa.action.label.slice(0, 6),
      icon: pa.action.icon,
      action: { type: "plugin", plugin: pa.plugin.id, action: pa.action.id },
    });
  };

  const save = async () => {
    try {
      await onSave(items);
      setStatus({ ok: true, text: "已保存，下次唤出即生效" });
    } catch (e) {
      setStatus({ ok: false, text: `保存失败：${e}` });
    }
  };

  const reset = async () => {
    try {
      setItems(await onReset());
      setFocus(null);
      setStatus({ ok: true, text: "已恢复默认" });
    } catch (e) {
      setStatus({ ok: false, text: `恢复失败：${e}` });
    }
  };

  return (
    <section className="editor">
      <div className="editor-head">
        <h2>
          轮盘方向{" "}
          <span className="muted">
            {items.length} / {MAX_ITEMS}，每项方位固定，新增一项只会切分它两侧相邻项之间的范围
          </span>
        </h2>
        <div className="editor-actions">
          {status && <span className={status.ok ? "ok" : "err"}>{status.text}</span>}
          <button onClick={reset}>恢复默认</button>
          <button className="primary" disabled={!dirty} onClick={save}>
            保存
          </button>
        </div>
      </div>

      <div className="editor-body">
        <div className="editor-preview">
          <div style={{ width: WHEEL_SIZE, height: WHEEL_SIZE, position: "relative" }}>
            <Wheel source={source} items={items} />
            {ghosts.map((a) => {
              const [x, y] = polar(L.r, a);
              return (
                <button
                  key={a}
                  className={`ghost ${pending === a ? "on" : ""}`}
                  style={{ left: x, top: y, width: L.w * 0.7, height: L.h * 0.7 }}
                  title={`在「${directionName(a)}」添加`}
                  onClick={() => setPending(a)}
                >
                  +
                </button>
              );
            })}
          </div>
        </div>

        <div className="editor-list">
          {items.map((it, i) => (
            <div
              key={it.id}
              className={`editor-row ${focus === i ? "focus" : ""}`}
              onFocus={() => setFocus(i)}
              onMouseEnter={() => setFocus(i)}
            >
              <select
                className="dir-select"
                value={it.angle}
                title="方位"
                onChange={(e) => {
                  const angle = Number(e.target.value);
                  const id = it.id;
                  setItems((list) => list.map((x) => (x.id === id ? { ...x, angle } : x)));
                  setFocus(null);
                }}
              >
                {DIRECTIONS.map((d) => (
                  <option
                    key={d.angle}
                    value={d.angle}
                    disabled={d.angle !== it.angle && items.some((x) => slot(x.angle) === slot(d.angle))}
                  >
                    {d.name}
                  </option>
                ))}
              </select>
              <IconPicker value={it.icon} onChange={(icon) => update(i, { icon })} />
              <input
                className="label-input"
                value={it.label}
                maxLength={6}
                onChange={(e) => update(i, { label: e.target.value })}
                placeholder="名称"
              />
              <select
                value={it.action.type}
                onChange={(e) => {
                  const t = e.target.value as ActionType;
                  const first = pluginActions[0];
                  update(i, {
                    action:
                      t === "plugin" && first
                        ? { type: "plugin", plugin: first.plugin.id, action: first.action.id }
                        : newAction(t),
                  });
                }}
              >
                {(Object.keys(ACTION_TYPES) as ActionType[])
                  .filter((t) => t !== "plugin" || pluginActions.length > 0 || it.action.type === "plugin")
                  .map((t) => (
                    <option key={t} value={t}>
                      {ACTION_TYPES[t].label}
                    </option>
                  ))}
              </select>
              {it.action.type === "ask" && (
                <input
                  className="extra-input"
                  value={it.action.prompt}
                  onChange={(e) => update(i, { action: { type: "ask", prompt: e.target.value } })}
                  placeholder="提问（留空则让 AI 解释截图）"
                />
              )}
              {it.action.type === "open" && (
                <input
                  className="extra-input"
                  value={it.action.target}
                  onChange={(e) => update(i, { action: { type: "open", target: e.target.value } })}
                  placeholder="https://… 或 /Applications/…"
                />
              )}
              {it.action.type === "plugin" && (
                <select
                  className="extra-input"
                  value={`${it.action.plugin}::${it.action.action}`}
                  onChange={(e) => {
                    const pa = pluginActions.find((x) => x.key === e.target.value);
                    if (pa) update(i, { action: { type: "plugin", plugin: pa.plugin.id, action: pa.action.id } });
                  }}
                >
                  {!pluginActions.some((x) => it.action.type === "plugin" && x.key === `${it.action.plugin}::${it.action.action}`) && (
                    <option value={`${it.action.plugin}::${it.action.action}`}>（插件未安装）</option>
                  )}
                  {pluginActions.map((x) => (
                    <option key={x.key} value={x.key}>
                      {x.plugin.name} · {x.action.label}
                    </option>
                  ))}
                </select>
              )}
              {it.action.type !== "ask" && it.action.type !== "open" && it.action.type !== "plugin" && (
                <span className="extra-input" />
              )}
              <div className="row-buttons">
                <button
                  title="删除"
                  disabled={items.length <= 1}
                  onClick={() => {
                    setItems((list) => list.filter((_, j) => j !== i));
                    setFocus(null);
                  }}
                >
                  ✕
                </button>
              </div>
            </div>
          ))}

          <div className="editor-add">
            <span className="muted">添加到</span>
            <select
              className="dir-select"
              value={pending ?? ""}
              disabled={pending === null}
              onChange={(e) => setPending(Number(e.target.value))}
            >
              {DIRECTIONS.filter((d) => !items.some((x) => slot(x.angle) === slot(d.angle))).map((d) => (
                <option key={d.angle} value={d.angle}>
                  {d.name}
                </option>
              ))}
            </select>
            {pluginActions.map((x) => (
              <button
                key={x.key}
                disabled={items.length >= MAX_ITEMS || pending === null}
                onClick={() => addPlugin(x.key)}
                title={x.plugin.description}
              >
                + {x.action.label}
              </button>
            ))}
            {(Object.keys(ACTION_TYPES) as ActionType[]).filter((t) => t !== "plugin").map((t) => (
              <button key={t} disabled={items.length >= MAX_ITEMS || pending === null} onClick={() => add(t)}>
                + {ACTION_TYPES[t].label}
              </button>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

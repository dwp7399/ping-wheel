import { useEffect, useState } from "react";
import { Icon } from "./icons";
import { api, on, timeText } from "./api";
import { getConfig, onConfigChanged, resetConfig, setConfig } from "./tauriSource";
import WheelEditor from "./WheelEditor";
import { CollectionsPage, MarksPage, ModelPage } from "./pages";
import { WheelItem } from "./types";

type ActionEvent = {
  item: WheelItem | null;
  at: number;
  status: "done" | "pending" | "failed";
  detail: string;
};

const STATUS_TEXT = { done: "已执行", pending: "处理中", failed: "失败" };

const TABS = [
  { id: "marks", label: "标记", icon: "mark" },
  { id: "collections", label: "收藏", icon: "collect" },
  { id: "wheel", label: "轮盘", icon: "menu" },
  { id: "model", label: "模型", icon: "sparkle" },
  { id: "log", label: "记录", icon: "note" },
] as const;
type Tab = (typeof TABS)[number]["id"];

export default function MainWindow() {
  const [tab, setTab] = useState<Tab>(() => {
    try {
      return (localStorage.getItem("main-tab") as Tab) || "marks";
    } catch {
      return "marks";
    }
  });
  const [events, setEvents] = useState<ActionEvent[]>([]);
  const [items, setItems] = useState<WheelItem[] | null>(null);

  useEffect(() => {
    getConfig().then((c) => setItems(c.items));
    const offs = [
      onConfigChanged((c) => setItems(c.items)),
      on<ActionEvent>("action", (e) => setEvents((list) => [e, ...list].slice(0, 50))),
    ];
    return () => offs.forEach((f) => f());
  }, []);

  const pick = (t: Tab) => {
    setTab(t);
    try {
      localStorage.setItem("main-tab", t);
    } catch {
      /* 预览环境可能不可用 */
    }
  };

  return (
    <div className="shell">
      <nav className="side">
        <div className="brand">Ping Wheel</div>
        {TABS.map((t) => (
          <button key={t.id} className={`side-item ${tab === t.id ? "on" : ""}`} onClick={() => pick(t.id)}>
            <Icon name={t.icon} size={17} />
            {t.label}
          </button>
        ))}
        <button className="side-item" onClick={() => api.chatOpenWindow()}>
          <Icon name="chat" size={17} />
          新对话
        </button>
        <div className="side-hint muted">⌘⇧C 唤出轮盘，指向后点击执行；右键或 Esc 取消</div>
      </nav>

      <main className="main">
        {tab === "marks" && <MarksPage />}
        {tab === "collections" && <CollectionsPage />}
        {tab === "model" && <ModelPage />}
        {tab === "wheel" && items && (
          <WheelEditor
            initial={items}
            onSave={(next) => setConfig({ items: next })}
            onReset={() => resetConfig().then((c) => c.items)}
          />
        )}
        {tab === "log" && (
          <section>
            <div className="page-head">
              <h2>
                最近触发 <span className="muted">本次启动以来</span>
              </h2>
            </div>
            {events.length === 0 ? (
              <p className="muted empty">还没有触发记录。</p>
            ) : (
              <ul className="events">
                {events.map((e) => (
                  <li key={e.at}>
                    <span className="event-name">
                      <Icon name={e.item?.icon ?? "menu"} size={16} /> {e.item?.label ?? "菜单"}
                    </span>
                    <span className={`muted s-${e.status}`}>
                      {e.detail} · {STATUS_TEXT[e.status]} · {timeText(e.at)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        )}
      </main>
    </div>
  );
}

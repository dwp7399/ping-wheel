import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Icon } from "./icons";
import { getConfig, onConfigChanged, resetConfig, setConfig } from "./tauriSource";
import WheelEditor from "./WheelEditor";
import { WheelItem } from "./types";

type ActionEvent = {
  item: WheelItem | null;
  at: number;
  status: "done" | "pending" | "failed";
  detail: string;
};

const STATUS_TEXT = { done: "已执行", pending: "开发中", failed: "失败" };

export default function MainWindow() {
  const [events, setEvents] = useState<ActionEvent[]>([]);
  const [items, setItems] = useState<WheelItem[] | null>(null);

  useEffect(() => {
    getConfig().then((c) => setItems(c.items));
    const offCfg = onConfigChanged((c) => setItems(c.items));
    const off = listen<ActionEvent>("action", (e) => {
      setEvents((list) => [e.payload, ...list].slice(0, 30));
    });
    return () => {
      offCfg();
      off.then((f) => f());
    };
  }, []);

  return (
    <main className="main">
      <header>
        <h1>Ping Wheel</h1>
        <p className="muted">
          按住 ⌘⇧C 唤出轮盘，在触控板上滑向某个方向，松开 C 执行。轻点 ⌘⇧C 可常驻，再按一次确认，Esc
          取消。
        </p>
      </header>

      {items && (
        <WheelEditor
          initial={items}
          onSave={(next) => setConfig({ items: next })}
          onReset={() => resetConfig().then((c) => c.items)}
        />
      )}

      <section>
        <h2>最近触发</h2>
        {events.length === 0 ? (
          <p className="muted">还没有触发记录。</p>
        ) : (
          <ul className="events">
            {events.map((e) => (
              <li key={e.at}>
                <span className="event-name">
                  <Icon name={e.item?.icon ?? "menu"} size={16} /> {e.item?.label ?? "菜单"}
                </span>
                <span className="muted">
                  {e.detail} · {STATUS_TEXT[e.status]} · {new Date(e.at).toLocaleTimeString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

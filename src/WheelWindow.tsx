import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Wheel from "./Wheel";
import { getConfig, onConfigChanged, tauriSource } from "./tauriSource";
import { DEFAULT_CONFIG, WheelItem } from "./types";

/** wheel 窗口：读取配置并在配置变化时刷新 */
export default function WheelWindow() {
  const [items, setItems] = useState<WheelItem[]>(DEFAULT_CONFIG.items);

  useEffect(() => {
    getConfig()
      .then((c) => setItems(c.items))
      .catch((e) => console.error("读取轮盘配置失败", e));
    // 没有事件监听权限时，窗口自己接收点击：左键执行，右键取消
    const onDown = (e: MouseEvent) => {
      e.preventDefault();
      invoke("wheel_click", { button: e.button }).catch((err) => console.error("轮盘点击失败", err));
    };
    const noMenu = (e: Event) => e.preventDefault();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("contextmenu", noMenu);
    const off = onConfigChanged((c) => setItems(c.items));
    return () => {
      off();
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("contextmenu", noMenu);
    };
  }, []);

  return <Wheel source={tauriSource} items={items} />;
}

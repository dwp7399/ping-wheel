import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { WheelSource } from "./Wheel";
import type { Selection, WheelConfig } from "./types";

export const tauriSource: WheelSource = (h) => {
  const offs = [
    listen("wheel:open", () => h.open()),
    listen<{ dx: number; dy: number; index: Selection }>("wheel:move", (e) =>
      h.move(e.payload.dx, e.payload.dy, e.payload.index),
    ),
  ];
  return () => offs.forEach((p) => p.then((f) => f()));
};

export const getConfig = () => invoke<WheelConfig>("get_config");
export const setConfig = (config: WheelConfig) => invoke<void>("set_config", { config });
export const resetConfig = () => invoke<WheelConfig>("reset_config");

export function onConfigChanged(cb: (c: WheelConfig) => void) {
  const off = listen<WheelConfig>("config:changed", (e) => cb(e.payload));
  return () => {
    off.then((f) => f());
  };
}

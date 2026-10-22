import { useEffect, useState } from "react";
import { Icon } from "./icons";
import type { IconName } from "./types";

export type ToastStatus = "done" | "pending" | "failed";
export type ToastData = { icon: IconName; title: string; detail: string; status: ToastStatus };

const STATUS_TEXT: Record<ToastStatus, string> = {
  done: "已执行",
  pending: "处理中",
  failed: "失败",
};

export type ToastSource = (show: (t: ToastData) => void, hide: () => void) => () => void;

/** 执行反馈提示：出现在唤出点，Rust 端负责定时隐藏窗口 */
export default function Toast({ source }: { source: ToastSource }) {
  const [data, setData] = useState<ToastData | null>(null);
  const [seq, setSeq] = useState(0);
  const [hiding, setHiding] = useState(false);

  useEffect(
    () =>
      source(
        (t) => {
          setData(t);
          setHiding(false);
          setSeq((n) => n + 1);
        },
        () => setHiding(true),
      ),
    [source],
  );

  if (!data) return null;
  return (
    <div className="toast-root">
      <div key={seq} className={`toast toast-${data.status} ${hiding ? "toast-hiding" : ""}`}>
        <div className="toast-icon">
          <Icon name={data.icon} size={20} />
        </div>
        <div className="toast-body">
          <div className="toast-title">
            {data.title}
            <span className="toast-badge">{STATUS_TEXT[data.status]}</span>
          </div>
          <div className="toast-detail">{data.detail}</div>
        </div>
      </div>
    </div>
  );
}

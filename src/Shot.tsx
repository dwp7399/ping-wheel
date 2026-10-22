import { useEffect, useState } from "react";
import { imageUrl } from "./api";

/** 显示一张截图（按需从 Rust 读取） */
export default function Shot({ path, className, onClick }: { path: string; className?: string; onClick?: () => void }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    imageUrl(path)
      .then((u) => alive && setSrc(u))
      .catch(() => alive && setSrc(null));
    return () => {
      alive = false;
    };
  }, [path]);
  return src ? (
    <img className={className} src={src} onClick={onClick} alt="截图" />
  ) : (
    <div className={`${className ?? ""} shot-placeholder`} />
  );
}

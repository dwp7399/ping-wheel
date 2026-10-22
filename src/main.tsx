import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
const inTauri = "__TAURI_INTERNALS__" in window;

async function boot() {
  // 先确定语言，再加载各界面（界面模块里的文案在加载时就会翻译）
  const { initLocale } = await import("./i18n");
  await initLocale(inTauri);
  if (!inTauri) {
    // 浏览器里打开 Vite 地址时进入预览模式，方便调界面
    document.documentElement.dataset.window = "preview";
    const view = new URLSearchParams(location.search).get("view");
    if (view === "toast") {
      const { default: Toast } = await import("./Toast");
      document.documentElement.dataset.window = "toast";
      const samples = [
        { icon: "mark", title: "标记", detail: "已标记：Rust 所有权与借用（可回到这个网页）", status: "done" },
        { icon: "link", title: "打开", detail: "已打开 https://github.com", status: "done" },
        { icon: "link", title: "打开", detail: "还没有设置要打开的目标，请在菜单里填写", status: "failed" },
      ] as const;
      let i = 0;
      root.render(
        <Toast
          source={(show) => {
            show({ ...samples[0] });
            const t = setInterval(() => show({ ...samples[++i % samples.length] }), 1500);
            return () => clearInterval(t);
          }}
        />,
      );
      return;
    }
    if (view === "main" || view === "panel") {
      const { installPreviewMocks } = await import("./previewMocks");
      installPreviewMocks(view === "panel" ? "panel-1" : "main");
      document.documentElement.dataset.window = view;
      if (view === "panel") {
        const { default: Panel } = await import("./Panel");
        root.render(<Panel />);
      } else {
        const { default: MainWindow } = await import("./MainWindow");
        root.render(<MainWindow />);
      }
      return;
    }
    if (view === "editor") {
      const [{ default: WheelEditor }, { DEFAULT_CONFIG }] = await Promise.all([
        import("./WheelEditor"),
        import("./types"),
      ]);
      root.render(
        <main className="main">
          <WheelEditor
            initial={DEFAULT_CONFIG.items}
            onSave={async () => {}}
            onReset={async () => DEFAULT_CONFIG.items}
          />
        </main>,
      );
      return;
    }
    const { default: Preview } = await import("./Preview");
    root.render(<Preview />);
    return;
  }

  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const label = getCurrentWindow().label;
  // panel-N / plugin-N 归为一类
  document.documentElement.dataset.window = label.replace(/-\d+$/, "");

  if (label === "toast") {
    const [{ default: Toast }, { listen }] = await Promise.all([
      import("./Toast"),
      import("@tauri-apps/api/event"),
    ]);
    root.render(
      <Toast
        source={(show, hide) => {
          const offs = [
            listen<import("./Toast").ToastData>("toast:show", (e) => show(e.payload)),
            listen("toast:hide", () => hide()),
          ];
          return () => offs.forEach((p) => p.then((f) => f()));
        }}
      />,
    );
  } else if (label.startsWith("panel-")) {
    const { default: Panel } = await import("./Panel");
    root.render(<Panel />);
  } else if (label.startsWith("plugin-")) {
    const { runPlugin } = await import("./PluginRunner");
    runPlugin();
  } else if (label === "wheel") {
    const { default: WheelWindow } = await import("./WheelWindow");
    root.render(<WheelWindow />);
  } else {
    const { default: MainWindow } = await import("./MainWindow");
    root.render(
      <React.StrictMode>
        <MainWindow />
      </React.StrictMode>,
    );
  }
}

boot();

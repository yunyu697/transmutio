import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import { themeToCss } from "@formatforge/design-tokens";
import { cleanupRetiredKeys } from "./lib/storage";

// alpha11 遗留键的一次性清理（只删两个确切键，无其它启动副作用）。
cleanupRetiredKeys();

// 设计令牌以 CSS 变量注入，深浅主题只切换 [data-theme]。
const tokens = document.createElement("style");
tokens.textContent = `:root{${themeToCss("dark")}}\n[data-theme="light"]{${themeToCss("light")}}`;
document.head.appendChild(tokens);

createRoot(document.getElementById("root")!).render(<App />);

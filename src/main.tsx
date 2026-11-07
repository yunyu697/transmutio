import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import { themeToCss } from "@formatforge/design-tokens";
import { cleanupRetiredKeys } from "./lib/storage";

// alpha11 遗留键的一次性清理：独立标记位控制只跑一次，不清扫其它任何键。
cleanupRetiredKeys();

// 设计令牌以 CSS 变量注入，深浅主题只切换 [data-theme]。
const tokens = document.createElement("style");
tokens.textContent = `:root{${themeToCss("dark")}}\n[data-theme="light"]{${themeToCss("light")}}`;
document.head.appendChild(tokens);

createRoot(document.getElementById("root")!).render(<App />);

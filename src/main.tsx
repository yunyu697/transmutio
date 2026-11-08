import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import { allThemeCss } from "@formatforge/design-tokens";
import { cleanupRetiredKeys } from "./lib/storage";

// alpha11 遗留键的一次性清理：独立标记位控制只跑一次，不清扫其它任何键。
cleanupRetiredKeys();

// 设计令牌以 CSS 变量注入：data-theme 管明暗，data-palette 管色板，两者正交。
const tokens = document.createElement("style");
tokens.textContent = allThemeCss();
document.head.appendChild(tokens);

createRoot(document.getElementById("root")!).render(<App />);

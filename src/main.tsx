import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import { themeToCss } from "@formatforge/design-tokens";

// 设计令牌以 CSS 变量注入，深浅主题只切换 [data-theme]。
const tokens = document.createElement("style");
tokens.textContent = `:root{${themeToCss("dark")}}\n[data-theme="light"]{${themeToCss("light")}}`;
document.head.appendChild(tokens);

createRoot(document.getElementById("root")!).render(<App />);

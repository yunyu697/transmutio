import { createUpdaterController, createNullUpdater } from "@formatforge/updater";

/**
 * 自动更新尚未接入真实端点（无签名密钥、无发布通道）。
 * 这里显式返回"未启用"，UI 会展示原因，不会假装检查成功。
 */
export const updater = createUpdaterController(
  createNullUpdater("自动更新尚未配置：缺少签名密钥与更新端点，暂不可用"),
  { enabled: false, disabledReason: "自动更新尚未配置：缺少签名密钥与更新端点，暂不可用" },
);

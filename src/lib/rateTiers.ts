/**
 * 码率与画质的「档位化」映射：界面只展示档位名 + 一句人话说明，
 * 具体数值（CRF / kbps）收进各面板的「高级」展开区。
 *
 * 内部仍然存数值：恒定质量存 CRF（后端原样传给 x264/x265 的 `-crf`），
 * 码率存 kbps。预设读回时按区间反解档位，所以区间边界集中在这里，
 * 不允许在界面里散落魔法数。
 */

export interface CrfTier {
  id: string;
  name: string;
  hint: string;
  /** 区间闭区间（越界值归入最近档） */
  min: number;
  max: number;
  /** 选中该档时写入的 CRF 值（= 区间中点附近，x264 常用取值） */
  defaultCrf: number;
}

/**
 * 五档映射（按 x264 / x265 的 CRF 标尺校准）：
 *   18 → 近乎无损；20 → 高质量；23 → 标准（x264 默认值）；
 *   26 → 省空间；30 → 极小体积。
 * 相邻两档间隔 ≥2 CRF，实测体积差约 25%–40%（见交付报告）。
 * x265 的 CRF 标尺比 x264 约「松 5 档」，同一档位在 HEVC 下体积更小、观感略降，
 * 但档位名与区间保持一致，避免同一界面对两套标尺。
 */
export const CRF_TIERS: CrfTier[] = [
  { id: "near-lossless", name: "近乎无损", hint: "体积最大，肉眼几乎看不出差别", min: 16, max: 18, defaultCrf: 18 },
  { id: "high", name: "高质量（推荐）", hint: "画质与体积平衡", min: 19, max: 21, defaultCrf: 20 },
  { id: "standard", name: "标准", hint: "满足日常观看", min: 22, max: 24, defaultCrf: 23 },
  { id: "small", name: "省空间", hint: "体积小，快速运动画面会略糊", min: 25, max: 28, defaultCrf: 26 },
  { id: "tiny", name: "极小体积", hint: "适合发群里或长期存档", min: 29, max: 34, defaultCrf: 30 },
];

export const CRF_MIN = CRF_TIERS[0].min;
export const CRF_MAX = CRF_TIERS[CRF_TIERS.length - 1].max;

/** 按 CRF 数值反解档位；越界归最近档。 */
export function tierForCrf(crf: number): CrfTier {
  const value = Math.round(crf);
  const exact = CRF_TIERS.find((tier) => value >= tier.min && value <= tier.max);
  if (exact) return exact;
  return value < CRF_MIN ? CRF_TIERS[0] : CRF_TIERS[CRF_TIERS.length - 1];
}

/** 视频目标码率档位（kbps）：标签口语化，数值只在高级里出现。 */
export interface BitrateTier {
  id: string;
  name: string;
  kbps: number;
}

export const VIDEO_BITRATE_TIERS: BitrateTier[] = [
  { id: "data-saver", name: "省流量", kbps: 1000 },
  { id: "web", name: "网页 / 社交分享", kbps: 2000 },
  { id: "standard", name: "标准", kbps: 4000 },
  { id: "hd", name: "高清", kbps: 8000 },
  { id: "high", name: "高码率", kbps: 16000 },
];

/** 音频码率档位（kbps） */
export const AUDIO_BITRATE_TIERS: BitrateTier[] = [
  { id: "standard", name: "标准", kbps: 128 },
  { id: "high", name: "高", kbps: 192 },
  { id: "higher", name: "更高", kbps: 256 },
  { id: "max", name: "极高", kbps: 320 },
];

export function tierForKbps(tiers: BitrateTier[], kbps: number): BitrateTier | null {
  return tiers.find((tier) => tier.kbps === Math.round(kbps)) ?? null;
}

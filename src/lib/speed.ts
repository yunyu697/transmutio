/** 变速范围与预设（与后端 args::validate_speed 保持一致）。 */
export const SPEED_MIN = 0.25;
export const SPEED_MAX = 4;
export const SPEED_STEP = 0.05;
export const SPEED_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2] as const;

export function speedLabel(value: number): string {
  return value === 1 ? "原速" : `${value}×`;
}

export function validateSpeed(raw: number): { ok: boolean; speed: number; error?: string } {
  if (!Number.isFinite(raw)) return { ok: false, speed: 1, error: "倍速必须是数字" };
  if (raw === 0) return { ok: true, speed: 1 };
  if (raw < SPEED_MIN || raw > SPEED_MAX) {
    return { ok: false, speed: 1, error: `倍速需在 ${SPEED_MIN}–${SPEED_MAX} 之间（1 表示不变）` };
  }
  return { ok: true, speed: raw };
}

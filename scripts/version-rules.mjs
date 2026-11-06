/**
 * 版本规则的唯一实现（供 CLI 与自测脚本共用）。
 *
 * 规则（与 Berthio 对齐）：
 * - 形如 `0.2.0` 或 `0.2.0-alpha3`；
 * - 除第一段外，其余每段数值 ≤ 10，`0.11.0` 直接判失败；
 * - alpha 序号为正整数，不带前导零（`alpha0` / `alpha01` 失败）。
 */

export const MAX_SEGMENT = 10;

/** 解析版本号；失败返回 { ok:false, reason }。 */
export function parseVersion(input) {
  if (typeof input !== "string" || input.trim() === "") {
    return { ok: false, reason: "版本号为空" };
  }
  const value = input.trim();
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-alpha(\d+))?$/.exec(value);
  if (!match) {
    return {
      ok: false,
      reason: `格式不合法：${value}（只接受 <major>.<minor>.<patch> 或 <major>.<minor>.<patch>-alpha<序号>）`,
    };
  }
  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);
  const alphaRaw = match[4];
  const alpha = alphaRaw === undefined ? null : Number(alphaRaw);
  const reasons = [];

  for (const [label, n] of [["minor", minor], ["patch", patch]]) {
    if (n > MAX_SEGMENT) {
      reasons.push(`${label} 段 ${n} 超过上限 ${MAX_SEGMENT}（例如 0.11.0 属于失败）`);
    }
  }
  if (major < 0) reasons.push("major 不能为负");
  if (alphaRaw !== undefined) {
    if (!/^[1-9]\d*$/.test(alphaRaw)) {
      reasons.push(`alpha 序号必须为正整数且不带前导零：alpha${alphaRaw}`);
    }
  }
  if (reasons.length > 0) return { ok: false, reason: reasons.join("；") };
  return {
    ok: true,
    value,
    major,
    minor,
    patch,
    alpha,
    isAlpha: alpha !== null,
    base: `${major}.${minor}.${patch}`,
  };
}

/** 校验 alpha 序号单调递增：新序号必须恰好是「已有同基线 tag 的最大序号 + 1」。 */
export function checkAlphaSequence(parsed, existingAlphaSequences) {
  if (!parsed.ok) return parsed;
  if (!parsed.isAlpha) return { ok: true };
  if (existingAlphaSequences.length === 0) {
    if (parsed.alpha !== 1) {
      return { ok: false, reason: `首个 alpha 必须是 alpha1，收到 alpha${parsed.alpha}` };
    }
    return { ok: true };
  }
  const max = Math.max(...existingAlphaSequences);
  if (parsed.alpha !== max + 1) {
    return {
      ok: false,
      reason: `alpha 序号必须单调递增：已有最大 alpha${max}，本次应为 alpha${max + 1}，收到 alpha${parsed.alpha}`,
    };
  }
  return { ok: true };
}

/** 从 tag 列表里取出同一基线的 alpha 序号；tag 形如 v0.2.0-alpha1。 */
export function collectAlphaSequences(tags, base) {
  const out = [];
  for (const tag of tags) {
    const m = new RegExp(`^v${base.replace(/\./g, "\\.")}-alpha(\\d+)$`).exec(tag.trim());
    if (m && /^[1-9]\d*$/.test(m[1])) out.push(Number(m[1]));
  }
  return out;
}

/** tag 必须等于 `v<version>`。 */
export function checkTag(parsed, tag) {
  if (!parsed.ok) return parsed;
  if (!tag) return { ok: true };
  const expected = `v${parsed.value}`;
  if (tag !== expected) {
    return { ok: false, reason: `tag 与版本不一致：tag=${tag}，期望 ${expected}` };
  }
  return { ok: true };
}

/** 产物 `--version` 输出必须与版本文件一致。 */
export function checkBinaryVersion(parsed, observedText) {
  if (!parsed.ok) return parsed;
  if (observedText === undefined || observedText === null || observedText === "") {
    return { ok: false, reason: "未能从产物读到版本" };
  }
  const out = String(observedText);
  // 接受 "Transmutio 0.2.0-alpha1" 或仅版本号两种输出
  const m = /(\d+\.\d+\.\d+(?:-alpha\d+)?)/.exec(out);
  if (!m) return { ok: false, reason: `产物输出中没有可识别的版本号：${out.trim()}` };
  if (m[1] !== parsed.value) {
    return { ok: false, reason: `产物版本与版本文件不一致：产物=${m[1]}，版本文件=${parsed.value}` };
  }
  return { ok: true, observed: m[1] };
}

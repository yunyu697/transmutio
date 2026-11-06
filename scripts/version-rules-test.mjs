#!/usr/bin/env node
/**
 * 版本规则自测：用反例证明校验能拦住不合规版本，并确认当前仓库版本合规。
 * 用法：node scripts/version-rules-test.mjs
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  parseVersion,
  checkAlphaSequence,
  collectAlphaSequences,
  checkTag,
  checkBinaryVersion,
} from "./version-rules.mjs";

let passed = 0;
const failures = [];

function expectReject(label, result) {
  if (result.ok) failures.push(`${label}：本应失败，却通过了`);
  else passed += 1;
  return result;
}
function expectAccept(label, result) {
  if (!result.ok) failures.push(`${label}：本应通过，却失败了（${result.reason}）`);
  else passed += 1;
}

// 1) 段位 ≤ 10：0.11.0 必须失败；0.10.0 / 0.2.10 允许
expectReject("0.11.0（minor=11）", parseVersion("0.11.0"));
expectReject("0.2.11（patch=11）", parseVersion("0.2.11"));
expectAccept("0.10.0（边界 10）", parseVersion("0.10.0"));
expectAccept("0.2.10（边界 10）", parseVersion("0.2.10"));

// 2) alpha 序号：alpha0 / alpha01 失败
expectReject("alpha0", parseVersion("0.2.0-alpha0"));
expectReject("alpha01（前导零）", parseVersion("0.2.0-alpha01"));

// 3) alpha 单调递增
expectAccept("首个 alpha1", checkAlphaSequence(parseVersion("0.2.0-alpha1"), []));
expectReject("首个别想直接 alpha2", checkAlphaSequence(parseVersion("0.2.0-alpha2"), []));
expectAccept("已有 alpha1 → alpha2", checkAlphaSequence(parseVersion("0.2.0-alpha2"), [1]));
expectReject("已有 alpha1 → 跳到 alpha3", checkAlphaSequence(parseVersion("0.2.0-alpha3"), [1]));
expectReject("回退到 alpha1（已有 alpha1/alpha2）", checkAlphaSequence(parseVersion("0.2.0-alpha1"), [1, 2]));

// 4) tag 与版本不一致
expectReject("tag v0.2.0-alpha2 对版本 0.2.0-alpha1", checkTag(parseVersion("0.2.0-alpha1"), "v0.2.0-alpha2"));
expectReject("tag 缺 v 前缀", checkTag(parseVersion("0.2.0-alpha1"), "0.2.0-alpha1"));
expectAccept("tag 一致", checkTag(parseVersion("0.2.0-alpha1"), "v0.2.0-alpha1"));

// 5) 产物版本与版本文件不一致
expectReject(
  "产物报 alpha0，文件是 alpha1",
  checkBinaryVersion(parseVersion("0.2.0-alpha1"), "Transmutio 0.2.0-alpha0"),
);
expectAccept(
  "产物与文件一致",
  checkBinaryVersion(parseVersion("0.2.0-alpha1"), "Transmutio 0.2.0-alpha1\n"),
);

// 6) collectAlphaSequences 能识别既有 tag
expectAccept(
  "collectAlphaSequences([v0.2.0-alpha1, v0.2.0-alpha2, v0.2.0])",
  collectAlphaSequences(["v0.2.0-alpha1", "v0.2.0-alpha2", "v0.2.0"], "0.2.0").join(",") === "1,2"
    ? { ok: true }
    : { ok: false, reason: "解析结果不符" },
);

// 7) 仓库当前版本必须合规
const version = JSON.parse(readFileSync(join(process.cwd(), "package.json"), "utf8")).version;
expectAccept(`仓库当前版本 ${version}`, parseVersion(version));

console.log(`[version-rules-test] 通过 ${passed} 项，失败 ${failures.length} 项`);
for (const f of failures) console.error(` - ${f}`);
process.exit(failures.length === 0 ? 0 : 1);

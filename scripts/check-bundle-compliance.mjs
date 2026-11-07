#!/usr/bin/env node
/**
 * 合规物解包检查：打开 dist-artifacts/<flavor>/ 里的真实安装包，
 * 断言许可证文本 / notices / 源码获取说明确实进了安装包（不是只看源码目录）。
 *
 * 同时验证「文案与行为一致」：安装包里不应出现 ffmpeg/ffprobe 二进制
 * （我们的声明是「不随包分发 FFmpeg，由用户自行获取」）。
 *
 * 用法：node scripts/check-bundle-compliance.mjs [lgpl|gpl|all]
 */
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { basename, extname, join } from "node:path";

const root = process.cwd();
const SEVEN_ZIP = "C:\\Program Files\\7-Zip\\7z.exe";

function findDark() {
  const base = join(process.env.LOCALAPPDATA ?? "", "tauri");
  if (!existsSync(base)) return null;
  for (const entry of readdirSync(base)) {
    const candidate = join(base, entry, "dark.exe");
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

const EXPECTED = {
  lgpl: {
    productName: "Transmutio",
    identifier: "app.transmutio.desktop",
    upgradeCode: "{5C1F7A2E-8D34-5B96-A1E7-C4D2F908B365}",
    required: [
      "README.txt",
      "LGPL-2.1.txt",
      "LGPL-3.0.txt",
      "THIRD-PARTY-NOTICES-LGPL.txt",
      "SOURCE-ACQUISITION-LGPL.txt",
    ],
    forbidden: ["GPL-2.0.txt", "THIRD-PARTY-NOTICES-GPL.txt", "SOURCE-ACQUISITION-GPL.txt"],
    sourceMarker: "不随包分发",
  },
  gpl: {
    productName: "Transmutio GPL",
    identifier: "app.transmutio.desktop.gpl",
    upgradeCode: "{B7E4D190-3C62-5A8F-9D2B-E6F1047C8A53}",
    required: [
      "README.txt",
      "GPL-2.0.txt",
      "GPL-3.0.txt",
      "THIRD-PARTY-NOTICES-GPL.txt",
      "SOURCE-ACQUISITION-GPL.txt",
    ],
    forbidden: ["LGPL-2.1.txt", "THIRD-PARTY-NOTICES-LGPL.txt", "SOURCE-ACQUISITION-LGPL.txt"],
    sourceMarker: "不随包分发",
  },
};

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

function sha256(path) {
  const hash = createHash("sha256");
  hash.update(readFileSync(path));
  return hash.digest("hex");
}

function powershellJson(script) {
  const encoded = Buffer.from(script, "utf16le").toString("base64");
  const out = spawnSync("powershell", ["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (out.status !== 0) {
    return { ok: false, error: (out.stderr || out.stdout || `exit ${out.status}`).trim().slice(-300) };
  }
  try {
    return { ok: true, data: JSON.parse(out.stdout.trim()) };
  } catch (error) {
    return { ok: false, error: `无法解析 PowerShell JSON：${error}` };
  }
}

/** 含反引号标识符（MSI SQL）的脚本必须用 -File 执行；-EncodedCommand 会抛 COMException。 */
function powershellJsonFile(script, tag) {
  const file = join(tmpdir(), `transmutio-${tag}.ps1`);
  writeFileSync(file, script, "utf8");
  const out = spawnSync("powershell", ["-NoProfile", "-NonInteractive", "-File", file], {
    encoding: "utf8",
    windowsHide: true,
  });
  rmSync(file, { force: true });
  if (out.status !== 0) {
    return { ok: false, error: (out.stderr || out.stdout || `exit ${out.status}`).trim().slice(-300) };
  }
  try {
    return { ok: true, data: JSON.parse(out.stdout.trim()) };
  } catch (error) {
    return { ok: false, error: `无法解析 PowerShell JSON：${error}` };
  }
}

/** 读取 MSI 的 ProductName / Manufacturer / UpgradeCode（Property 表）。 */
function msiIdentity(artifact) {
  const path = artifact.replace(/'/g, "''");
  const script = [
    "$ErrorActionPreference='Stop'",
    "$w=New-Object -ComObject WindowsInstaller.Installer",
    `$db=$w.GetType().InvokeMember('OpenDatabase','InvokeMethod',$null,$w,@('${path}',0))`,
    "$props=@{}",
    "foreach($n in @('ProductName','Manufacturer','UpgradeCode','ProductVersion')){",
    "  $q=\"SELECT Value FROM Property WHERE Property='$n'\"",
    "  $v=$db.GetType().InvokeMember('OpenView','InvokeMethod',$null,$db,@($q))",
    "  $v.GetType().InvokeMember('Execute','InvokeMethod',$null,$v,$null)",
    "  $r=$v.GetType().InvokeMember('Fetch','InvokeMethod',$null,$v,$null)",
    "  if($r){$props[$n]=$r.GetType().InvokeMember('StringData','GetProperty',$null,$r,@(1))}",
    "}",
    "$props | ConvertTo-Json -Compress",
  ].join("\n");
  return powershellJson(script);
}

/** 读取 NSIS 安装包 exe 的版本资源。 */
function exeVersionInfo(artifact) {
  const path = artifact.replace(/'/g, "''");
  const script = `(Get-Item -LiteralPath '${path}').VersionInfo | Select-Object ProductName,FileDescription,CompanyName,FileVersion | ConvertTo-Json -Compress`;
  return powershellJson(script);
}

function normalizeGuid(value) {
  return (value ?? "").trim().toUpperCase();
}

/** 旧版并存拦截：读 MSI 的 Upgrade.ActionProperty / LaunchCondition.Condition。 */
function msiLegacyGuard(artifact) {
  const p = artifact.replace(/'/g, "''");
  const script = [
    "$ErrorActionPreference='Stop'",
    "$bt=[char]96",
    `$path='${p}'`,
    "$w=New-Object -ComObject WindowsInstaller.Installer",
    "$db=$w.GetType().InvokeMember('OpenDatabase','InvokeMethod',$null,$w,@($path,0))",
    "$rows=@(); $lc=@()",
    "$v=$db.GetType().InvokeMember('OpenView','InvokeMethod',$null,$db,@('SELECT '+$bt+'UpgradeCode'+$bt+','+$bt+'ActionProperty'+$bt+' FROM Upgrade'))",
    "$v.GetType().InvokeMember('Execute','InvokeMethod',$null,$v,$null)",
    "while($true){$r=$v.GetType().InvokeMember('Fetch','InvokeMethod',$null,$v,$null); if($r -eq $null){break}; $rows += ($r.GetType().InvokeMember('StringData','GetProperty',$null,$r,@(1)) + '=>' + $r.GetType().InvokeMember('StringData','GetProperty',$null,$r,@(2)))}",
    "$v2=$db.GetType().InvokeMember('OpenView','InvokeMethod',$null,$db,@('SELECT '+$bt+'Condition'+$bt+' FROM LaunchCondition'))",
    "$v2.GetType().InvokeMember('Execute','InvokeMethod',$null,$v2,$null)",
    "while($true){$r=$v2.GetType().InvokeMember('Fetch','InvokeMethod',$null,$v2,$null); if($r -eq $null){break}; $lc += $r.GetType().InvokeMember('StringData','GetProperty',$null,$r,@(1))}",
    "@{upgrade=$rows;conditions=$lc} | ConvertTo-Json -Compress",
  ].join("\n");
  return powershellJsonFile(script, "bundle-guard");
}

const LEGACY_GUARD = {
  lgpl: { upgradeCode: "{68CDE348-14E5-52EE-8A32-0972D9760143}", property: "OLD_FORMATFORGE_LGPL" },
  gpl: { upgradeCode: "{D0BEF367-4F7D-57E2-AC29-60145E1DB4F0}", property: "OLD_FORMATFORGE_GPL" },
};
const LEGACY_LAUNCH_CONDITION = "NOT OLD_FORMATFORGE_LGPL AND NOT OLD_FORMATFORGE_GPL";

function extract(artifact, targetDir) {
  rmSync(targetDir, { recursive: true, force: true });
  mkdirSync(targetDir, { recursive: true });
  const ext = extname(artifact).toLowerCase();
  if (ext === ".exe") {
    // NSIS：7-Zip 可直接解包
    const out = spawnSync(SEVEN_ZIP, ["x", "-y", `-o${targetDir}`, artifact], { encoding: "utf8" });
    return { method: "7z (NSIS)", ok: out.status === 0, log: (out.stdout || out.stderr || "").split(/\r?\n/).slice(-3).join(" | ") };
  }
  if (ext === ".msi") {
    // MSI：WiX dark.exe 反编译（不需要管理员权限，不写系统 Installer 状态）。
    // dark 把载荷按 MSI 的 File Id 命名解出，真实文件名在同时生成的 .wxs 的 <File Id=... Name=.../> 里。
    const dark = findDark();
    if (!dark) {
      return { method: "dark.exe", ok: false, log: "未找到 WiX dark.exe（%LOCALAPPDATA%\\tauri\\WixTools*\\dark.exe）" };
    }
    const parent = join(targetDir, "..");
    mkdirSync(parent, { recursive: true });
    const out = spawnSync(dark, ["-nologo", "-x", targetDir, artifact], { encoding: "utf8", cwd: parent });
    const wxs = join(parent, `${basename(artifact).replace(/\.msi$/i, "")}.wxs`);
    const nameMap = {};
    const appUserModelIds = [];
    if (existsSync(wxs)) {
      const text = readFileSync(wxs, "utf8");
      for (const m of text.matchAll(/<File\s+Id="([^"]+)"\s+Name="([^"]+)"/g)) {
        nameMap[m[1]] = basename(m[2].replace(/\\/g, "/"));
      }
      for (const m of text.matchAll(/<ShortcutProperty[^>]*Key="System\.AppUserModel\.ID"[^>]*Value="([^"]+)"/g)) {
        appUserModelIds.push(m[1]);
      }
      rmSync(wxs, { force: true });
    }
    return {
      method: "dark.exe (WiX 反编译)",
      ok: out.status === 0,
      log: `exit=${out.status}；File 表映射 ${Object.keys(nameMap).length} 项；identifier ${appUserModelIds.length} 项`,
      nameMap,
      appUserModelIds,
    };
  }
  return { method: "unsupported", ok: false, log: `不支持的安装包类型 ${ext}` };
}

function checkFlavor(flavor) {
  const distDir = join(root, "dist-artifacts", flavor);
  const spec = EXPECTED[flavor];
  const results = [];
  const artifacts = walk(distDir).filter((p) => [".exe", ".msi"].includes(extname(p).toLowerCase()));
  if (artifacts.length === 0) {
    results.push({
      name: `${flavor}：找到安装包`,
      status: "fail",
      detail: `dist-artifacts/${flavor} 下没有 .exe/.msi；先运行 npm run build:${flavor}`,
    });
    return results;
  }
  for (const artifact of artifacts) {
    const name = basename(artifact);
    const targetDir = join(root, "samples", "bundle-extract", flavor, name.replace(/[^\w.-]+/g, "_"));
    const ex = extract(artifact, targetDir);
    const nameMap = ex.nameMap ?? {};
    const files = walk(targetDir).map((p) => ({
      path: p,
      raw: basename(p),
      // MSI 的载荷按 File Id 落盘，这里映射回真实文件名；NSIS 直接用原名。
      name: nameMap[basename(p)] ?? basename(p),
      rel: p.slice(targetDir.length + 1),
      size: statSync(p).size,
    }));
    const foundNames = new Set(files.map((f) => f.name));
    const missing = spec.required.filter((n) => !foundNames.has(n));
    const forbiddenPresent = spec.forbidden.filter((n) => foundNames.has(n));
    const ffBinaries = files.filter((f) => /^(ffmpeg|ffprobe)(\.exe)?$/i.test(f.name));
    const sourceFile = files.find((f) => f.name === `SOURCE-ACQUISITION-${flavor.toUpperCase()}.txt`);
    let sourceMarkerOk = false;
    let sourceMarkerDetail = "未找到源码获取说明";
    if (sourceFile) {
      const text = readFileSync(sourceFile.path, "utf8");
      sourceMarkerOk = text.includes(spec.sourceMarker) && text.includes("源码");
      sourceMarkerDetail = `含「${spec.sourceMarker}」=${text.includes(spec.sourceMarker)}，长度=${text.length}`;
    }
    const licenseFiles = files.filter((f) => /^L?GPL-\d\.\d\.txt$/.test(f.name));
    const licenseTextOk = licenseFiles.length >= 1 && licenseFiles.every((f) => f.size > 10_000);
    results.push({
      name: `${flavor}：${name} 解包`,
      status: ex.ok ? "pass" : "fail",
      detail: `${ex.method} → ${files.length} 个文件；${ex.log}`,
      evidence: { artifact, size: statSync(artifact).size, sha256: sha256(artifact), extracted: files.length },
    });
    results.push({
      name: `${flavor}：${name} 含全部许可证/notices/源码说明`,
      status: missing.length === 0 && forbiddenPresent.length === 0 && sourceMarkerOk && licenseTextOk ? "pass" : "fail",
      detail: `缺失=${missing.join(",") || "无"}；不该出现=${forbiddenPresent.join(",") || "无"}；${sourceMarkerDetail}；许可证文本≥10KB=${licenseTextOk}`,
      evidence: {
        found: spec.required.filter((n) => foundNames.has(n)),
        missing,
        forbiddenPresent,
        licenseFiles: licenseFiles.map((f) => ({ name: f.name, size: f.size })),
      },
    });
    results.push({
      name: `${flavor}：${name} 未随包分发 ffmpeg 二进制（文案与行为一致）`,
      status: ffBinaries.length === 0 ? "pass" : "fail",
      detail: ffBinaries.length === 0 ? "未发现 ffmpeg.exe/ffprobe.exe" : `发现 ${ffBinaries.map((f) => f.rel).join(", ")}`,
      evidence: { ffBinaries: ffBinaries.map((f) => f.rel) },
    });
    const devBins = files.filter((f) => /-(check|report)\.exe$/i.test(f.name));
    results.push({
      name: `${flavor}：${name} 未混入验证用二进制（dev-tools 未进包）`,
      status: devBins.length === 0 ? "pass" : "fail",
      detail: devBins.length === 0 ? "未发现 *-check.exe / flavor-report.exe" : `发现 ${devBins.map((f) => f.rel).join(", ")}`,
      evidence: { devBins: devBins.map((f) => f.rel) },
    });

    // 应用 exe 必须内嵌 Windows manifest（declares comctl32 6.0 依赖）。
    // 缺了它，TaskDialogIndirect 无法从 comctl32 5.82 解析，启动即「无法定位程序输入点」。
    const expectedExe = new RegExp(`^${spec.productName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\.exe$`, "i");
    const appExe = files.find((f) => expectedExe.test(f.name));
    let manifestOk = false;
    let manifestDetail = `未找到 ${spec.productName}.exe`;
    if (appExe) {
      const text = readFileSync(appExe.path).toString("latin1");
      const assembly = text.includes("<assembly");
      const commonControls = text.includes("Microsoft.Windows.Common-Controls");
      manifestOk = assembly && commonControls;
      manifestDetail = `manifest=${assembly}；comctl32 v6 依赖=${commonControls}`;
    }
    results.push({
      name: `${flavor}：${name} 应用 exe 内嵌 Windows manifest（comctl32 6.0）`,
      status: manifestOk ? "pass" : "fail",
      detail: manifestDetail,
      evidence: { appExe: appExe?.rel ?? null, size: appExe?.size ?? null },
    });

    // 主程序文件名必须是产品名，且不得残留旧名 formatforge.exe。
    const legacyExe = files.find((f) => /^formatforge\.exe$/i.test(f.name));
    results.push({
      name: `${flavor}：${name} 主程序文件名（${spec.productName}.exe，无旧名残留）`,
      status: appExe && !legacyExe ? "pass" : "fail",
      detail: appExe
        ? `主程序=${appExe.rel}；旧名残留=${legacyExe ? legacyExe.rel : "无"}`
        : `未找到 ${spec.productName}.exe；包内 exe=${files.filter((f) => /\.exe$/i.test(f.name)).map((f) => f.name).join(", ") || "无"}`,
      evidence: { appExe: appExe?.rel ?? null, legacyExe: legacyExe?.rel ?? null },
    });

    // 产品身份：MSI 读 Property/ShortcutProperty；NSIS 读 exe 版本资源 + 归档的 installer.nsi。
    const ext = extname(artifact).toLowerCase();
    if (ext === ".msi") {
      const identity = msiIdentity(artifact);
      const data = identity.ok ? identity.data : {};
      const identityOk =
        identity.ok &&
        data.ProductName === spec.productName &&
        normalizeGuid(data.UpgradeCode) === normalizeGuid(spec.upgradeCode);
      results.push({
        name: `${flavor}：${name} 产品身份（ProductName / UpgradeCode）`,
        status: identityOk ? "pass" : "fail",
        detail: identity.ok
          ? `ProductName=${data.ProductName}；UpgradeCode=${data.UpgradeCode}；期望=${spec.productName} / ${spec.upgradeCode}`
          : identity.error,
        evidence: { productName: data.ProductName ?? null, upgradeCode: data.UpgradeCode ?? null },
      });
      const appUserModelIds = ex.appUserModelIds ?? [];
      results.push({
        name: `${flavor}：${name} identifier（反编译 ShortcutProperty System.AppUserModel.ID）`,
        status: appUserModelIds.includes(spec.identifier) ? "pass" : "fail",
        detail: `实际=${appUserModelIds.join(", ") || "无"}；期望=${spec.identifier}`,
        evidence: { identifiers: appUserModelIds },
      });

      // 旧版并存拦截：MSI 必须靠 Upgrade(OnlyDetect) + LaunchCondition 中止安装。
      const guard = msiLegacyGuard(artifact);
      const rows = guard.ok
        ? Array.isArray(guard.data.upgrade)
          ? guard.data.upgrade
          : guard.data.upgrade
            ? [guard.data.upgrade]
            : []
        : [];
      const conditions = guard.ok
        ? Array.isArray(guard.data.conditions)
          ? guard.data.conditions
          : guard.data.conditions
            ? [guard.data.conditions]
            : []
        : [];
      const guardOk =
        guard.ok &&
        rows.some((r) => String(r).includes(LEGACY_GUARD[flavor].upgradeCode) && String(r).includes(LEGACY_GUARD[flavor].property)) &&
        conditions.some((c) => String(c).includes(LEGACY_LAUNCH_CONDITION));
      results.push({
        name: `${flavor}：${name} 旧版并存拦截（Upgrade OnlyDetect + LaunchCondition）`,
        status: guardOk ? "pass" : "fail",
        detail: guard.ok
          ? `Upgrade：${rows.join(" ; ") || "无"}；LaunchCondition：${conditions.join(" ; ") || "无"}`
          : guard.error,
        evidence: { upgrade: rows, conditions },
      });
    } else if (ext === ".exe") {
      const version = exeVersionInfo(artifact);
      const data = version.ok ? version.data : {};
      results.push({
        name: `${flavor}：${name} 版本资源 ProductName`,
        status: version.ok && data.ProductName === spec.productName ? "pass" : "fail",
        detail: version.ok
          ? `ProductName=${data.ProductName}；FileDescription=${data.FileDescription}；期望=${spec.productName}`
          : version.error,
        evidence: data,
      });
      const nsiPath = join(distDir, "installer.generated.nsi");
      const nsiText = existsSync(nsiPath) ? readFileSync(nsiPath, "utf8") : "";
      const nsiOk =
        nsiText.includes(`!define PRODUCTNAME "${spec.productName}"`) &&
        nsiText.includes(`!define BUNDLEID "${spec.identifier}"`);
      results.push({
        name: `${flavor}：${name} 生成脚本身份（PRODUCTNAME / BUNDLEID）`,
        status: nsiOk ? "pass" : "fail",
        detail: existsSync(nsiPath)
          ? `installer.generated.nsi：PRODUCTNAME/BUNDLEID 与期望一致=${nsiOk}`
          : "缺少 installer.generated.nsi（重建后由 build-flavor.mjs 归档）",
        evidence: { nsi: existsSync(nsiPath) ? nsiPath : null },
      });

      // 旧版并存拦截：NSIS 必须真正调用 NSIS_HOOK_PREINSTALL（Tauri v2 唯一入口），
      // 且 hooks 文件里定义了该宏（只定义不调用会被模板静默忽略，实测踩过）。
      const hookFile = join(root, "src-tauri", "installer-hooks.nsh");
      const hookText = existsSync(hookFile) ? readFileSync(hookFile, "utf8") : "";
      const hookInvoked = nsiText.includes("!insertmacro NSIS_HOOK_PREINSTALL");
      const hookDefined = /!macro NSIS_HOOK_PREINSTALL/.test(hookText);
      results.push({
        name: `${flavor}：${name} 旧版并存拦截（NSIS_HOOK_PREINSTALL 已调用且已定义）`,
        status: hookInvoked && hookDefined ? "pass" : "fail",
        detail: `生成脚本调用=${hookInvoked}；hooks 定义=${hookDefined}；旧版检测细节见 npm run verify:legacy-guard`,
        evidence: { hookInvoked, hookDefined },
      });
    }
  }
  return results;
}

function main() {
  const arg = (process.argv[2] || "all").toLowerCase();
  const flavors = arg === "all" ? ["lgpl", "gpl"] : [arg];
  const report = {
    generatedAt: new Date().toISOString(),
    level: "安装包解包检查（真实产物；不依赖窗口）",
    sevenZip: existsSync(SEVEN_ZIP) ? SEVEN_ZIP : null,
    flavors: {},
    summary: { passed: 0, failed: 0, total: 0 },
  };
  for (const flavor of flavors) {
    const results = checkFlavor(flavor);
    report.flavors[flavor] = { results };
    for (const r of results) {
      report.summary.total += 1;
      if (r.status === "pass") report.summary.passed += 1;
      else report.summary.failed += 1;
    }
  }
  mkdirSync(join(root, "samples"), { recursive: true });
  writeFileSync(join(root, "samples", "compliance-check.json"), JSON.stringify(report, null, 2), "utf8");
  for (const flavor of flavors) {
    for (const r of report.flavors[flavor].results) {
      console.log(`${r.status === "pass" ? "PASS" : "FAIL"}  ${r.name} — ${r.detail}`);
    }
  }
  console.log(`\n合计 ${report.summary.passed}/${report.summary.total} PASS → samples/compliance-check.json`);
  process.exit(report.summary.failed === 0 ? 0 : 1);
}

main();

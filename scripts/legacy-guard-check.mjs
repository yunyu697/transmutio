#!/usr/bin/env node
/**
 * 旧版并存拦截的可复现验证（Transmutio）。
 *
 * 覆盖两条安装链路：
 *  1) MSI（WiX）：读最终安装包的 LaunchCondition / AppSearch / RegLocator，
 *     断言「命中旧 FormatForge UpgradeCode 即中止」确实编进了安装数据库。
 *     证据等级 = 静态核验（数据库级，不是反编译猜测）。
 *  2) NSIS：静态核验归档的 installer.generated.nsi 是否真的调用 NSIS_HOOK_PREINSTALL；
 *     并用同一份 installer-hooks.nsh 编译一个**不弹窗**的探针 exe，静默运行，
 *     在真实注册表上执行同一段检测代码，把结果写入临时文件。
 *     证据等级 = 实测（本机真实注册表，无 UI、无安装）。
 *
 * 说明：MSI 不做运行时安装探针——per-machine MSI 会触发 UAC 提权并写宿主系统状态，
 * 违反本项目「不装到宿主」的纪律；改用数据库级静态证据。
 *
 * 用法：node scripts/legacy-guard-check.mjs [lgpl|gpl|all]
 * 输出：samples/legacy-guard-report.json
 */
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = process.cwd();
const LEGACY_UPGRADE_CODES = {
  lgpl: "{68CDE348-14E5-52EE-8A32-0972D9760143}", // 旧 FormatForge LGPL
  gpl: "{D0BEF367-4F7D-57E2-AC29-60145E1DB4F0}", // 旧 FormatForge GPL
};
const LEGACY_PACKED_KEYS = {
  lgpl: "843EDC865E41EE25A82390279D671034", // 上者的 MSI 打包键名（注册表用）
  gpl: "763FEB0DD7F42E75CA920641E5D14B0F",
};
const HOOK_MACRO = "NSIS_HOOK_PREINSTALL";
const NSIS_DIR = join(process.env.LOCALAPPDATA ?? "", "tauri", "NSIS");

const flavors = (() => {
  const arg = (process.argv[2] || "all").toLowerCase();
  if (arg === "all") return ["lgpl", "gpl"];
  if (arg === "lgpl" || arg === "gpl") return [arg];
  console.error(`未知变体：${arg}`);
  process.exit(2);
})();

/** 用临时 .ps1 执行（-EncodedCommand 在含反引号标识符的脚本上会抛 COMException）。 */
function runPs1Text(script, tag) {
  const file = join(tmpdir(), `transmutio-${tag}.ps1`);
  writeFileSync(file, script, "utf8");
  const out = spawnSync("powershell", ["-NoProfile", "-NonInteractive", "-File", file], {
    encoding: "utf8",
    windowsHide: true,
  });
  rmSync(file, { force: true });
  if (out.status !== 0) return { ok: false, error: (out.stderr || out.stdout || "").trim().slice(-400) };
  return { ok: true, text: out.stdout.trim() };
}

/** MSI 数据库级核验：拦截用的 Upgrade / LaunchCondition 表是否存在、是否指向旧 UpgradeCode。 */
function inspectMsi(msiPath) {
  const p = msiPath.replace(/'/g, "''");
  const ps = [
    "$ErrorActionPreference = 'Stop'",
    "$bt = [char]96",
    `$path = '${p}'`,
    "$w = New-Object -ComObject WindowsInstaller.Installer",
    "$db = $w.GetType().InvokeMember('OpenDatabase','InvokeMethod',$null,$w,@($path,0))",
    "function Fetch([string]$sql) {",
    "  $v = $db.GetType().InvokeMember('OpenView','InvokeMethod',$null,$db,@($sql))",
    "  $v.GetType().InvokeMember('Execute','InvokeMethod',$null,$v,$null)",
    "  while ($true) {",
    "    $r = $v.GetType().InvokeMember('Fetch','InvokeMethod',$null,$v,$null)",
    "    if ($null -eq $r) { break }",
    "    $cols = @()",
    "    for ($i = 1; $i -le 5; $i++) {",
    "      try { $cols += $r.GetType().InvokeMember('StringData','GetProperty',$null,$r,@($i)) } catch { $cols += '' }",
    "    }",
    "    Write-Output ($cols -join '|')",
    "  }",
    "}",
    "Write-Output 'UPGRADE:'",
    "Fetch ('SELECT ' + $bt + 'UpgradeCode' + $bt + ',' + $bt + 'ActionProperty' + $bt + ' FROM Upgrade')",
    "Write-Output 'LC:'",
    "Fetch ('SELECT ' + $bt + 'Condition' + $bt + ' FROM LaunchCondition')",
  ].join("\n");
  const result = runPs1Text(ps, "msi-guard");
  if (!result.ok) return { ok: false, error: result.error };
  const lines = result.text.split(/\r?\n/).filter(Boolean);
  const cut = (tag) => {
    const start = lines.indexOf(tag);
    if (start < 0) return [];
    const rest = lines.slice(start + 1);
    const next = rest.findIndex((l) => /^(UPGRADE|LC):$/.test(l));
    return next < 0 ? rest : rest.slice(0, next);
  };
  const upgrade = cut("UPGRADE:");
  const launchConditions = cut("LC:");
  return {
    ok: true,
    data: { hasUpgrade: upgrade.length > 0, hasLaunchCondition: launchConditions.length > 0, upgrade, launchConditions },
  };
}

/** NSIS 运行时探针：复用 installer-hooks.nsh 的检测代码，不弹窗、不安装。 */
function probeNsis(flavor) {
  const makensis = join(NSIS_DIR, "makensis.exe");
  if (!existsSync(makensis)) return { ok: false, error: `未找到 makensis.exe（${NSIS_DIR}）` };
  const hooks = resolve(root, "src-tauri", "installer-hooks.nsh");
  const workDir = resolve(root, "samples", "_legacy_guard_probe", flavor);
  rmSync(workDir, { recursive: true, force: true });
  mkdirSync(workDir, { recursive: true });
  const resultFile = join(workDir, "result.txt");
  const exeFile = join(workDir, "probe.exe");
  const nsi = [
    `Name "TransmutioLegacyGuardProbe-${flavor}"`,
    `OutFile "${exeFile.replace(/\\/g, "\\\\")}"`,
    "RequestExecutionLevel user",
    "SilentInstall silent",
    "AutoCloseWindow true",
    "ShowInstDetails nevershow",
    "ShowUninstDetails nevershow",
    `!include "${hooks.replace(/\\/g, "\\\\")}"`,
    'Section "-probe"',
    `  FileOpen $0 "${resultFile.replace(/\\/g, "\\\\")}" w`,
    '  StrCpy $R9 "0"',
    "  SetRegView 64",
    "  !insertmacro ScanUninstallRoot HKLM 64",
    "  !insertmacro ScanUninstallRoot HKCU 64",
    "  SetRegView 32",
    "  !insertmacro ScanUninstallRoot HKLM 32",
    "  !insertmacro ScanUninstallRoot HKCU 32",
    "  SetRegView lastused",
    "  Call CheckLegacyUpgradeKeys",
    '  FileWrite $0 "hit=$R9$\\r$\\n"',
    "  SetRegView 64",
    `  EnumRegValue $R2 HKLM "Software\\Classes\\Installer\\UpgradeCodes\\${LEGACY_PACKED_KEYS[flavor]}" 0`,
    '  FileWrite $0 "hklm_value=$R2$\\r$\\n"',
    "  SetRegView lastused",
    "  FileClose $0",
    "SectionEnd",
  ].join("\n");
  const nsiPath = join(workDir, "probe.nsi");
  writeFileSync(nsiPath, nsi, "utf8");
  const build = spawnSync(makensis, ["/V2", nsiPath], { encoding: "utf8", windowsHide: true });
  if (build.status !== 0) {
    return { ok: false, error: `makensis 失败：${(build.stdout || build.stderr || "").slice(-300)}` };
  }
  const run = spawnSync(exeFile, [], { encoding: "utf8", windowsHide: true, timeout: 30_000 });
  const text = existsSync(resultFile) ? readFileSync(resultFile, "utf8") : "";
  return {
    ok: text.length > 0,
    exitCode: run.status,
    result: text.trim().split(/\r?\n/).filter(Boolean),
    hit: /hit=1/.test(text),
    workDir,
  };
}

const asArray = (value) => (Array.isArray(value) ? value : value ? [value] : []);

/** 按前缀找最新 MSI，不写死版本号（版本升级后脚本仍然可用）。 */
function findMsi(dir, prefix) {
  if (!existsSync(dir)) return "";
  const files = readdirSync(dir).filter((f) => f.startsWith(prefix) && f.endsWith(".msi"));
  if (!files.length) return "";
  files.sort((a, b) => statSync(join(dir, b)).mtimeMs - statSync(join(dir, a)).mtimeMs);
  return join(dir, files[0]);
}

const report = { generatedAt: new Date().toISOString(), flavors: {} };
let failures = 0;

for (const flavor of flavors) {
  const distDir = join(root, "dist-artifacts", flavor);
  const prefix = flavor === "gpl" ? "Transmutio GPL" : "Transmutio";
  const msi = findMsi(distDir, `${prefix}_`);
  const nsiPath = join(distDir, "installer.generated.nsi");
  const hooksPath = join(root, "src-tauri", "installer-hooks.nsh");
  const legacyUpgradeCode = LEGACY_UPGRADE_CODES[flavor];
  const entry = {};
  // 1) MSI
  if (existsSync(msi)) {
    const m = inspectMsi(msi);
    const upgrade = asArray(m.data?.upgrade).join(" | ");
    const lc = asArray(m.data?.launchConditions).join(" | ");
    const ok =
      m.ok &&
      m.data.hasUpgrade === true &&
      m.data.hasLaunchCondition === true &&
      upgrade.includes(legacyUpgradeCode) &&
      upgrade.includes(`OLD_FORMATFORGE_${flavor === "gpl" ? "GPL" : "LGPL"}`) &&
      /NOT OLD_FORMATFORGE_LGPL AND NOT OLD_FORMATFORGE_GPL/.test(lc);
    if (!ok) failures += 1;
    entry.msi = {
      status: ok ? "pass" : "fail",
      method: "静态核验（MSI 数据库表：Upgrade / LaunchCondition）",
      upgrade,
      launchCondition: lc,
      error: m.ok ? undefined : m.error,
    };
  } else {
    entry.msi = { status: "fail", detail: `未找到 ${msi}` };
    failures += 1;
  }

  // 2) NSIS 静态
  const nsiText = existsSync(nsiPath) ? readFileSync(nsiPath, "utf8") : "";
  const hooksText = existsSync(hooksPath) ? readFileSync(hooksPath, "utf8") : "";
  const invoked = new RegExp(`!insertmacro ${HOOK_MACRO}`).test(nsiText);
  const defined = new RegExp(`!macro ${HOOK_MACRO}`).test(hooksText);
  if (!(invoked && defined)) failures += 1;
  entry.nsisStatic = {
    status: invoked && defined ? "pass" : "fail",
    method: "静态核验（归档的 installer.generated.nsi + installer-hooks.nsh）",
    invoked,
    defined,
    hookFileIncluded: /installer-hooks\.nsh/.test(nsiText),
  };

  // 3) NSIS 运行时探针
  const probe = probeNsis(flavor);
  if (!(probe.ok && probe.hit)) failures += 1;
  entry.nsisProbe = {
    status: probe.ok && probe.hit ? "pass" : "fail",
    method: "实测（同一 hooks 源文件编译成无 UI 探针，静默跑本机真实注册表）",
    exitCode: probe.exitCode,
    result: probe.result,
    error: probe.error,
  };

  report.flavors[flavor] = entry;
}

report.summary = `${failures === 0 ? "PASS" : "FAIL"}：${failures} 项失败`;
mkdirSync(join(root, "samples"), { recursive: true });
writeFileSync(join(root, "samples", "legacy-guard-report.json"), JSON.stringify(report, null, 2), "utf8");

for (const [flavor, entry] of Object.entries(report.flavors)) {
  console.log(`\n[${flavor}]`);
  for (const [name, item] of Object.entries(entry)) {
    console.log(`  ${item.status === "pass" ? "PASS" : "FAIL"}  ${name}`);
    if (item.status !== "pass") console.log(`        ${JSON.stringify(item)}`);
  }
  if (entry.msi.launchCondition) console.log(`        LaunchCondition: ${entry.msi.launchCondition}`);
}
console.log(`\n${report.summary}`);
process.exit(failures === 0 ? 0 : 1);

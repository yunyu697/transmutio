# 改名方案：FormatForge → Transmutio（**已执行**）

> 状态：**已执行（2026-11-05）**。正式名 `Transmutio`；本文件保留为方案与执行记录。
>
> 执行结果摘要（详细证据见 `MVP-完成度与验证.md` §4.8/§4.9）：
>
> - 两变体产品名 / identifier / 窗口标题 / 更新通道 / 安装目录 / 快捷方式 / ARP / **安装包内可执行文件名**
>   （`Transmutio.exe` / `Transmutio GPL.exe`，由 `mainBinaryName` 控制）全部切换为 Transmutio；
>   MSI 新 UpgradeCode：LGPL `{5C1F7A2E-8D34-5B96-A1E7-C4D2F908B365}`、GPL `{B7E4D190-3C62-5A8F-9D2B-E6F1047C8A53}`。
> - 数据迁移（`src-tauri/src/migration.rs`）：同卷临时目录 → 文件数/目录数/字节数/逐文件 SHA-256 校验 →
>   原子 rename → 写 `migration.json` / `migration.log`；失败只清临时目录；另有设置页手动入口。
>   **该迁移功能已在 1.0.0-alpha12 整体移除**（`migration.rs` 删除、设置页入口删除、前端旧键不读不迁移），
>   下文相关章节仅作历史记录；旧版并存拦截仍然保留。
> - 旧版并存拦截：NSIS 走 `NSIS_HOOK_PREINSTALL`（ARP 显示名前缀 + 旧 UpgradeCode 打包键），
>   MSI 走 `Upgrade(OnlyDetect)` + `LaunchCondition`；`npm run verify:legacy-guard` 6/6 PASS
>   （NSIS 为真机注册表实测，MSI 为数据库级静态核验）。
> - `npm run check:bundle` 由 20 条断言扩到 **36/36 PASS**（新增产品身份、identifier、版本资源、
>   生成脚本身份、旧版拦截、主程序文件名共 16 条）。
> - 本轮踩坑记录（避免复发）：
>   1. WiX 3.14 的 `light` 会**静默丢弃**没有被任何 Feature 引用的 Fragment —— 必须用
>      `bundle.windows.wix.componentGroupRefs` 引用一个（可为空的）ComponentGroup 才会真正链接；
>   2. `Installer\UpgradeCodes\<packed>` 下旧产品码是**值**不是子键，NSIS 必须用 `EnumRegValue`；
>   3. Tauri v2 的 NSIS 钩子宏名是 `NSIS_HOOK_PREINSTALL`，`customInit` 不会被调用；
>   4. MSI 数据库代码页 1252，`LaunchCondition` 的 Description 只能用 ASCII。

---

## 1. 命名结构

| 变体 | 产品名（productName） | 说明 |
| --- | --- | --- |
| 默认（LGPL） | `Transmutio` | 不带后缀，ARP / 快捷方式 / 窗口标题 / 安装目录都用它 |
| GPL 变体 | `Transmutio GPL` | 后缀表示 FFmpeg 许可策略，不表示功能子集 |

**建议保留 `基础名 + " GPL"` 结构，不要改成 `Transmutio (GPL Edition)` 或缩写。** 理由：

- 现有两变体的差异是 FFmpeg 许可与编码器可用性，`GPL` 是最短且准确的区分词；
- 安装包文件名、ARP 列表、开始菜单排序都以产品名为准，短后缀不会把两个变体推到列表两端；
- 未来若再出现第三种构建（例如 nonfree 实验版），沿用 `Transmutio XXX` 后缀即可扩展；
- 不建议用 `Transmutio LGPL` 作为默认变体名：默认版是用户第一眼看到的产品，不应在名字里先强调许可限制。

窗口标题建议 `Transmutio`（GPL 版 `Transmutio GPL`），表示中文定位的「格式工坊」降为副标题或直接去掉，避免品牌名被长尾词稀释。

## 2. identifier（应用标识）

| 变体 | 新 identifier | 旧 identifier（仅迁移/拦截代码引用） |
| --- | --- | --- |
| 默认 | `app.transmutio.desktop` | `app.formatforge.desktop` |
| GPL | `app.transmutio.desktop.gpl` | `app.formatforge.desktop.gpl` |

约束：

- 两个变体必须继续不同，否则 tools 目录、日志、崩溃记录会共享（现有隔离设计不破坏）；
- `app.transmutio.desktop` 是 Tauri `identifier`、`%APPDATA%` 数据目录名、WebView2 用户数据目录名、NSIS/MSI 注册表键的共同来源，**只改这一处源头**（`tauri.conf.json` / `tauri.gpl.conf.json` / `flavor.rs::app_identifier`），不要在代码里散落写死；
- 改名后旧 identifier 只允许出现在两处：**数据迁移代码**与**旧版拦截代码**（外加对应测试）。

## 3. 数据目录迁移

### 3.1 涉及的三类目录

| 类别 | 旧路径 | 新路径 | 是否迁移 |
| --- | --- | --- | --- |
| 应用数据（tools / logs / crash.log） | `%APPDATA%\app.formatforge.desktop*` | `%APPDATA%\app.transmutio.desktop*` | **必须迁移**（同变体对同变体） |
| WebView2 用户数据（localStorage：设置 + 面板参数） | `%LOCALAPPDATA%\app.formatforge.desktop*\EBWebView` | `%LOCALAPPDATA%\app.transmutio.desktop*\EBWebView` | **必须迁移**，否则用户设置与面板参数全部回到默认 |
| 安装目录（程序本体） | `%LOCALAPPDATA%\Programs\FormatForge*` 或 `Program Files\FormatForge*` | 由新 productName 自动决定 | 不迁移，旧版由用户卸载 |

> WebView2 目录的实际位置以 Tauri v2 / WebView2 运行时默认值为准，实施前先在干净机器上确认一次，再写进迁移代码常量。

迁移只在**同变体**之间发生：旧 LGPL → 新默认版；旧 GPL → 新 `Transmutio GPL`。跨变体不迁移（沿用现有「两变体数据隔离」的结论）。

### 3.2 迁移算法（同卷临时目录 → 校验 → 原子落位 → 旧目录保留）

前置条件：

1. 旧目录存在；新目录不存在或为空。新目录已存在（非空）→ 视为已迁移，直接使用，不覆盖。
2. 旧版进程未运行。安装器的「旧版并存拦截」（§5）保证迁移发生在旧版已卸载之后；首启检测到旧进程/旧安装仍在时**不迁移**，只提示。
3. 迁移只在应用启动早期、初始化 tools 之前执行，且用迁移锁（名字固定的 lock 文件 + 已存在则等待/跳过）避免二次启动并发迁移。

步骤：

1. **快照源**：递归枚举旧目录，记录条目数、文件数、每文件字节数、总字节数；同时记录目录结构（空目录也要记）。
2. **同卷临时目录**：在目标父目录下创建 `.transmutio-migrate-<yyyyMMddHHmmss>-<rand>`（与目标同卷，保证第 5 步 rename 是原子操作，不跨卷）。
3. **复制**：逐文件复制到临时目录，保留相对路径与空目录；不复制符号链接/重解析点（遇到即失败，避免把系统目录带进来）。
4. **校验**：重新枚举临时目录，逐项比对：
   - 文件数、目录数一致；
   - 每文件大小一致、总字节数一致；
   - 关键配置（设置文件、`tools/` 下二进制）额外比对 SHA-256；
   - 任一项不一致 → 进入失败路径。
5. **原子落位**：`rename(临时目录, 新目录)`。Windows 同卷 rename 是原子的；目标必须不存在（已在第 1 步保证）。rename 返回「目标已存在」时按「已迁移」处理，不重试覆盖。
6. **旧目录保留**：不删除、不改名、不移动；新目录写一份 `migrated-from.txt`（源路径、时间、变体、文件数、字节数），便于支持与回滚。
7. WebView2 目录用**同一套步骤**单独迁移；失败不影响应用启动，只让设置回到默认，并记录一条诊断日志（下次启动重试）。

失败语义（必须写进实现与测试）：

- **旧目录完全不动**：不删、不改、不截断；
- **新目录不出现半套**：失败时只可能留下临时目录，且立即尽力删除（带重试）；删不掉则记录路径并告警，绝不 rename 成正式目录；
- 迁移失败不阻止应用启动；工具目录缺失会回落到首次引导页（现有行为），设置缺失回落到默认值。

### 3.3 迁移测试（改名轮必须补）

- 正常迁移：造多层目录 + 空目录 + 大/小文件，断言新目录条目数、字节数、关键文件哈希与源一致；
- 目标已存在的幂等：第二次启动不重复迁移，旧目录仍保留；
- 复制失败注入（复制中途删掉临时文件 / 目标父目录只读）→ 断言旧目录哈希不变、新目录不存在、临时目录已清理；
- 校验失败注入（临时目录多写 1 字节）→ 同上；
- 跨变体不迁移：旧 LGPL 数据存在时启动 GPL 版，断言新 GPL 目录不被写入旧数据。

## 4. 更新通道名

现状：`lgpl-stable` / `gpl-stable`（`flavor.rs::update_channel`，并已写进构建报告与文档）。

**建议改为 `transmutio-lgpl-stable` / `transmutio-gpl-stable`。** 理由：

- 当前更新器是 Null 实现（无端点、无密钥），改名成本为零，不存在已发布清单需要兼容；
- `lgpl-stable` / `gpl-stable` 这种通用名一旦接入共享更新服务，容易与其它产品撞名；
- 前缀产品名后，通道语义不变（`-stable` 仍表示稳定通道），也保留了许可区分。

若未来要加预览通道，按 `transmutio-<flavor>-<channel>` 扩展（如 `transmutio-lgpl-beta`），不要另起命名体系。

## 5. 安装 / 卸载与旧版并存

### 5.1 ARP 与快捷方式

| 项 | 旧值 | 新值 |
| --- | --- | --- |
| ARP DisplayName | `FormatForge 0.1.0` / `FormatForge GPL 0.1.0` | `Transmutio 0.1.0` / `Transmutio GPL 0.1.0` |
| Publisher / copyright | `FormatForge` | `Transmutio` |
| 开始菜单快捷方式 | `FormatForge.lnk` / `FormatForge GPL.lnk` | `Transmutio.lnk` / `Transmutio GPL.lnk` |
| 安装目录 | 由 productName 派生 | 由新 productName 派生（与旧版目录不同） |
| MSI ProductName / UpgradeCode | 旧产品身份 | **新产品身份**（新 UpgradeCode；不与旧 MSI 共享升级链） |
| NSIS 卸载注册表键 | 由旧身份派生 | 由新身份派生（键名以模板实际写入为准，不手写猜键） |

结论：旧版与新版在系统层面是**两个不同产品**，默认可以并存（不同安装目录、不同 ARP、不同数据目录）。这不是我们想要的用户体验，因此必须显式拦截。

### 5.2 旧版并存拦截（两层）

**第一层：安装器拦截（主拦截）**

- NSIS：安装开始时读取 `HKCU/HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\*`，匹配 DisplayName 以 `FormatForge` 开头或 InstallLocation/UninstallString 含 `app.formatforge.desktop`；命中 → 弹窗「检测到旧版 FormatForge，请先卸载后再安装 Transmutio」并中止安装（不自动卸载、不迁移）。
- MSI：用 WiX RegistrySearch（32/64 位各一份）+ LaunchCondition 实现同样的中止；MSI 不擅自定义交互，文案写进 LaunchCondition 的错误提示即可。
- 反向（旧版装到新版机器上）不在控制范围：旧安装包已经发布，无法修改；靠「新旧 identifier 不同、不会互相覆盖」+ 文档说明「先卸载新版再装旧版会产生并存」。

**第二层：应用启动检查（防御性）**

- 启动时扫描 ARP，命中旧版 → 顶部横幅「检测到旧版 FormatForge。两版数据目录不同，请卸载旧版后使用 Transmutio」，并**跳过自动迁移**（避免旧版还在运行时复制其数据）；
- 旧版不存在 → 正常执行 §3 迁移；
- 该检查只提示，不阻止启动，避免误判锁死用户。

### 5.3 卸载

- 卸载 `Transmutio` 只删自己的安装目录、快捷方式、ARP、`app.transmutio.desktop*`（是否删用户数据按现有策略：默认保留，提供显式「同时删除设置」选项）；
- **不触碰** `app.formatforge.desktop*` 旧目录与旧版残留（迁移后旧目录按要求保留）；
- 卸载日志里写明「旧版数据仍保留在 %APPDATA%\app.formatforge.desktop，可手动删除」。

## 6. `formatforge` 字符串白名单

改名时最容易出的事故是「全局替换」误伤内部标识。按下面两张表处理。

### 6.1 必须保留（内部标识，用户不可见；改名的收益低、破坏面大）

| 字符串 | 位置 | 保留理由 |
| --- | --- | --- |
| ~~`formatforge:settings`、`formatforge:panel:*`~~ | 前端 localStorage 键 | **已废弃（alpha12）**：只读写 `transmutio:*`；旧前缀残留键不读取、不迁移、不删除 |
| `FORMATFORGE_ALLOW_SHUTDOWN` | 关机动作环境变量 | **实际决定**：本轮不改名、不加 `TRANSMUTIO_ALLOW_SHUTDOWN` 别名。它是安全闸门，改主名会让既有脚本静默失效（关机动作从「需要显式开启」变成「永远关闭」或误开），旧名保留最稳。用户可见处只在设置页用来说明「需设置该变量」，属于开关名而非品牌名 |
| `FORMATFORGE_ALLOW_GUI_E2E`（若仍在使用） | 验证脚本 | 同上，内部开关 |
| `@formatforge/task-queue`、`@formatforge/design-tokens`、`@formatforge/settings`、`@formatforge/i18n`、`@formatforge/updater` | monorepo 包名 | 内部依赖名，不进用户界面；机械改名可另开一轮，避免和品牌改名混在一起 |
| `formatforge`（Cargo package）、`formatforge_lib`、`formatforge.exe`（仅 `cargo build` 调试产物） | `src-tauri/Cargo.toml`、构建脚本 | 简介：用户可见的可执行文件名已用 `mainBinaryName` 改为 `Transmutio.exe` / `Transmutio GPL.exe`；保留的只是编译期包名/库名与调试产物名，改它要牵动 `main.rs`、4 个 dev-tools bin、`build.rs`、验证脚本与增量缓存，收益低 |
| `headless-check` / `setup-check` / `flavor-check` / `flavor-report` | dev-tools bin | 内部验证程序，不进安装包 |
| `FF_FLAVOR`、`samples/*`、`scripts/*` 中内部路径 | build.rs / 验证脚本 | 纯内部 |
| `app.formatforge.desktop*` | **仅限**迁移代码、旧版拦截代码、迁移/拦截测试、历史文档 | 这是「旧身份」的事实来源，删了就没法迁移 |
| `FormatForge` | 历史版本说明、CHANGELOG、旧包文件名记录、`RENAME-PLAN.md` 自身 | 历史不可改写 |

### 6.2 必须修改（用户可见或对外身份）

| 字符串 | 位置 |
| --- | --- |
| `productName`、`identifier`、窗口 `title` | `src-tauri/tauri.conf.json`、`src-tauri/tauri.gpl.conf.json` |
| `FormatForge` / `FormatForge GPL` 产品名常量 | `src-tauri/src/flavor.rs`（`product_name`、`app_identifier`、`update_channel`） |
| 顶部品牌、设置页版本块、引导页文案、GPL 提示 | `src/components/TopBar.tsx`、`SettingsSheet.tsx`、`SetupWizard.tsx` |
| `app.name` 中英文案 | `src/lib/settings.ts` |
| 诊断导出标题 | `src/lib/telemetry.ts` |
| 浏览器标题 | `index.html` |
| `shortDescription` / `longDescription` / `copyright` | `tauri.conf.json` bundle 段 |
| 安装包文件名、ARP DisplayName、快捷方式 | 由 productName 自动派生（构建后核对） |
| 更新通道 | `flavor.rs::update_channel` |
| 文档：`README.md`、`LICENSING.md`、`MVP-完成度与验证.md` | 品牌与身份表格 |
| 测试断言里的可见产品名/identifier/通道 | `scripts/setup-ui-check.mjs`、`scripts/panel-persist-check.mjs`（fixture 的 `productName` / `appIdentifier` / `updateChannel`）；Rust 侧 `flavor-check.rs`、`setup_check.rs` 若断言了这些字符串，同步改为新值（迁移测试除外） |
| 日志路径 | 由 identifier 派生（`appData/logs`、`appData/crash.log`），随 identifier 自动变化；文档中写死的旧路径要改 |

> 执行顺序建议：先改配置与常量 → 全局搜索 `FormatForge` 逐条对照白名单 → 再改文案与文档 → 最后改测试断言。禁止无差别 `sed` 全仓替换。

## 7. 需要改动的全部触点清单

| # | 触点 | 改动 |
| --- | --- | --- |
| 1 | `src-tauri/tauri.conf.json` | `productName` → `Transmutio`；`identifier` → `app.transmutio.desktop`；窗口 `title`；bundle 三段文案；`copyright` |
| 2 | `src-tauri/tauri.gpl.conf.json` | `productName` → `Transmutio GPL`；`identifier` → `app.transmutio.desktop.gpl` |
| 3 | `src-tauri/src/flavor.rs` | `product_name` / `app_identifier` / `update_channel` 三个常量；单测同步 |
| 4 | `src-tauri/Cargo.toml` | **不改** package/lib 名（白名单）；仅确认无产品名写死 |
| 5 | `package.json` | `name` 保持内部名（白名单）；检查 scripts 中是否引用产品名做输出路径（有则改） |
| 6 | `src-tauri/src/lib.rs` | 崩溃日志路径已由 identifier 派生，无需改逻辑；核对注释与文档字符串 |
| 7 | `src-tauri/src/jobs.rs`、`after.rs`、`setup.rs` | 路径均由 `app_data_dir()` 派生，无需改逻辑；环境变量名（关机）新增主名 + 旧名兼容 |
| 8 | `src/lib/settings.ts` | `app.name` 中英文案；localStorage 前缀**保留** |
| 9 | `src/lib/panelPrefs.ts` | 前缀**保留** |
| 10 | `src/lib/telemetry.ts` | 诊断标题品牌名 |
| 11 | `src/components/TopBar.tsx`、`SettingsSheet.tsx`、`SetupWizard.tsx` | 品牌、版本块、GPL 文案 |
| 12 | `index.html` | `<title>` |
| 13 | `scripts/setup-ui-check.mjs`、`panel-persist-check.mjs` | 变体 fixture 与期望文案；若断言「FormatForge GPL」需改 |
| 14 | `scripts/build-flavor.mjs` | 产物目录/文件名由 productName 派生，逻辑不变；核对报告字段 |
| 15 | `scripts/check-bundle.mjs` | 解包目标文件名与产品名字符串需更新 |
| 16 | `README.md`、`LICENSING.md`、`MVP-完成度与验证.md` | 品牌、身份表、路径、通道 |
| 17 | 新增：安装器脚本/模板（NSIS hook、WiX LaunchCondition） | 旧版 ARP 检测与中止 |
| 18 | 新增：迁移模块（Rust） | §3 算法 + 锁 + 失败清理 |
| 19 | 新增：启动横幅 | 旧版并存提示 |

## 8. 新旧并存的边界（明确写进用户文档）

- 旧版 `FormatForge` **不会被**新版 `Transmutio` 覆盖：identifier、productName、安装目录、Uninstall 键全不同，Windows 视为两个产品；
- 因此**允许**并存，但**不支持**并存：两版各自有独立数据目录，不会互相读写；同时运行可能抢同一个输出目录/同一个 ffmpeg 子进程配额；
- 正确路径是**先卸载旧版，再安装新版**；安装器会主动拦截并给出这句提示；
- 迁移只在新版首次启动、且旧版已不存在时发生；迁移后旧目录按要求保留，用户可手动删除；
- 若用户先装新版、再装旧版，也会并存；旧包无法修改，只能在文档与支持口径里说明「请卸载新版或旧版，不要同时使用」。

## 9. 执行与验收顺序（下一轮）

1. 配置/常量改名 → 编译 → 产物名与 ARP 名核对；
2. 迁移模块 + 单测（正常/幂等/两类失败注入/跨变体不迁移）；
3. 安装器拦截（NSIS + MSI）→ 用真实旧版 ARP 记录做命中测试（只读注册表，不实际安装旧版）；
4. 前端文案与文档；
5. 全量回归：`cargo test`、`verify:core`、`verify:flavor ×2`、`verify:setup`、`verify:ui`、`check:bundle`；
6. 重新打包四个安装包，记录 size 与 SHA-256；旧包保留归档，不覆盖。

回滚：改名轮若中途放弃，只需回退配置/常量/文案与新增模块；数据迁移未执行前无副作用，执行后旧目录仍在，可把新目录改名回旧名恢复。

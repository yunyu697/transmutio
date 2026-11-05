# GPU / 硬件编解码能力矩阵与运行时探测

本文回答四件事：各平台到底有哪些硬编硬解路径、本机实测到什么、哪些只是文档依据、
以及代码里如何做到「探测而不是假设」。

> **证据分级（全文统一）**
> - **实测**：本机（Windows 11 + AMD RX 5700 XT）真实执行过命令并拿到输出。
> - **文档**：依据 FFmpeg 文档 / Apple VideoToolbox 文档 / 平台硬件文档，未在本机验证。
> - **待真机**：需要 macOS（Apple Silicon）/ Linux 设备才能确认，当前**未验证**。
>
> 本机 ffmpeg 为 `9.0.2-full_build-www.gyan.dev`（GPL，仅开发验证，不随包分发）。

---

## 1. 结论速览

| 平台 | 硬件编码路径 | 本状态 | 硬件解码 | 本状态 |
| --- | --- | --- | --- | --- |
| Windows + NVIDIA | `h264_nvenc` / `hevc_nvenc` / `av1_nvenc` | 编译存在；本机无 N 卡，试编失败 → 判定不可用（**实测失败**） | `cuda` / `d3d11va` / `dxva2` | `d3d11va` 真实样本解码通过（**实测**）；cuda 无设备；dxva2 未单独验证 |
| Windows + AMD | `h264_amf` / `hevc_amf` / `av1_amf` | `h264_amf`、`hevc_amf` 试编通过并真实转码（**实测**） | `d3d11va` / `dxva2` / `amf` | `d3d11va` 通过（**实测**）；dxva2 未单独验证 |
| Windows + Intel | `h264_qsv` / `hevc_qsv` / `av1_qsv` | 编译存在；本机试编失败 → 不可用（**实测失败**，无 Intel GPU） | `qsv` / `d3d11va` | 未验证（无设备） |
| macOS / Apple Silicon | `h264_videotoolbox` / `hevc_videotoolbox` | 编译存在与否取决于构建；无 NVENC/QSV/AMF（**文档**） | `videotoolbox` | **待真机** |
| Linux Intel / AMD | `h264_vaapi` / `hevc_vaapi` / `av1_vaapi`（及 QSV） | 已按文档补齐 `-vaapi_device` + `hwupload`（**文档**，未在 Linux 编译运行） | `vaapi` | **待真机** |

关键结论：**「编译进来」≠「能用」**。本机 ffmpeg 同时编译了 nvenc / qsv / vaapi / d3d12va，
但只有 AMF 真正可用。所以应用没有静态平台表，而是每次探测后按结果渲染。

---

## 2. Windows（实测）

### 2.1 编译支持 ≠ 设备可用

```
ffmpeg -hide_banner -hwaccels
→ cuda vaapi dxva2 qsv d3d11va opencl vulkan d3d12va amf
```

`-hwaccels` 只是编译期列表。逐项试编（320x240 testsrc2，0.6s）结果：

| 编码器 | 试编结果 | 判定 |
| --- | --- | --- |
| `h264_amf` | 通过 | 可用（硬编） |
| `hevc_amf` | 通过 | 可用（硬编） |
| `h264_nvenc` / `hevc_nvenc` | 失败 | 无 NVIDIA 设备 → `hw-unavailable` |
| `h264_qsv` / `hevc_qsv` | 失败 | 无 Intel iGPU → `hw-unavailable` |
| `h264_vaapi` / `hevc_vaapi` | 失败 | Windows 无 `/dev/dri` → 不可用 |
| `h264_d3d12va` | 失败 | 构建含该编码器但试编失败 → 不可用 |
| `libx264` / `libx265` | 通过 | 软件编码，但属 **GPL**，默认包禁用 |
| `libopenh264` | 不存在 | 该构建未集成（LGPL 软件 H.264 缺口，见 LICENSING.md） |

### 2.2 AMF 参数能力（来自 `ffmpeg -h encoder=h264_amf`，实测输出）

- 支持设备：`d3d11va`、`dxva2`、`amf`
- 支持像素格式：`nv12 yuv420p d3d11 dxva2_vld p010le amf bgr0 rgb0 bgra argb rgba x2bgr10le rgbaf16le`
- 码控：`-rc` 支持 `cqp / qvbr / hqvbr / hqcbr`（本项目用 `cqp` 恒定质量、`vbr_peak` 目标码率）
- 质量预设：`-quality`（`speed / balanced / quality / high_quality`，-1..3）
- 量化参数：`-qp_i` / `-qp_p` / `-qp_b`；B 帧参数 `-bf_delta_qp`

对应实现：`src-tauri/src/args.rs` 的 `rate_args()`，AMF 分支用
`-rc cqp -qp_i/-qp_p <q> -quality quality`。

### 2.3 硬件解码（实测）

探测流程是「先用可用硬编生成一个 1280x720 真实样本，再逐项 `-hwaccel` 解码并输出 null」，
避免只信 `-hwaccels` 列表：

- `d3d11va`：**实测通过**，并在真实 4K→1080p 转码里启用。
- `dxva2`：构建支持，未单独跑通过/失败结论，**未验证**。
- `cuda`：无 NVIDIA 设备，未验证。
- `d3d12va`：无可用设备，未验证。

### 2.4 并发与性能（实测）

- 同机 4 路 `h264_amf` 同时转码：4/4 成功，总耗时 6.6s（4 个 3s 1080p 源）。
- 单路 120s / 1080p → 720p：6.7s，约 17.9x（`d3d11va` 硬解 + AMF 硬编）。
- 4K（3840x2160）→1080p：成功；未单独记录纯硬编 vs 纯软编的对照耗时（**未测对照**）。

> 注意：AMF 在远程桌面会话、虚拟机、以及驱动异常时可能初始化失败。
> 这类失败在应用里归为 `hw-unavailable` 并回退软编，而不是直接判任务失败。

---

## 3. macOS / Apple Silicon（按文档实现，待真机验证）

**本机无 Apple 设备，以下全部未实测。** 结论按 FFmpeg 文档与 VideoToolbox 公开资料整理。

### 3.1 只有 VideoToolbox，没有 NVENC / QSV / AMF

- Apple Silicon（M 系列）没有独立 NVIDIA/Intel 编码器，也不提供 NVENC/QSV/AMF；
  硬件编解码统一经 **VideoToolbox**（`h264_videotoolbox` / `hevc_videotoolbox`），
  由 SoC 的媒体引擎承担。→ **文档依据，待真机确认编码器列表**
- Intel Mac 同样以 VideoToolbox 为主（部分机型另有 QSV），不作为默认假设。

真机核对命令（应用探测做的就是这些）：

```bash
ffmpeg -hide_banner -encoders | grep videotoolbox
ffmpeg -hide_banner -hwaccels
ffmpeg -hide_banner -h encoder=h264_videotoolbox
ffmpeg -hide_banner -h encoder=hevc_videotoolbox
```

### 3.2 已知限制与待核实点

| 项目 | 现状 / 结论 | 证据 |
| --- | --- | --- |
| 像素格式 | `h264_videotoolbox` 主要吃 `nv12` / `yuv420p`；`hevc_videotoolbox` 在 Apple Silicon 支持 `p010le` 10bit | **文档**；待 `-h encoder=` 核对 |
| 10bit / HEVC | HEVC Main10 与 HEVC 硬编需 macOS 11+（本项目 `minimumSystemVersion=11.0`）；M1 及以后支持 HEVC 硬编 | **文档**，**待真机** |
| B 帧 | 历史版本 `h264_videotoolbox` 对 B 帧支持有限；HEVC 在较新系统可用。本项目**不主动设置 `-bf`**，走编码器默认 | **待真机**（用 `-h encoder=` 看 `bf` 默认值，并用 `ffprobe -show_frames` 核对 `pict_type=B`） |
| 码控 | VT 没有 CRF 语义：质量模式用 `-q:v`（0..100，越大越好），码率模式用 `-b:v`。本项目 `vt_quality()` 是经验映射（CRF 23 ≈ q:v 55 一带），**待真机标定** | 实现见 `args.rs`；映射**未校准** |
| `allow_sw` | VideoToolbox 自身会在硬编不可用时悄悄转软件。本项目在 VT 分支显式 `-allow_sw 0`，让失败暴露出来，再由调度器分类回退 | **文档** + 实现 |
| 其他可用选项 | `-realtime`、`-prio_speed`、`-constant_bit_rate` 等由 FFmpeg 暴露，本项目未使用（默认即可） | **文档**，未验证效果 |
| 并发上限 | M 系列共享单一媒体引擎，多路硬编可能互相拖慢；本项目默认并发仍为 4，**未按平台下调** | **未验证** |
| 冷启动 | 首次硬编含 VideoToolbox 会话初始化，耗时/内存未测 | **未验证** |

> 因此 macOS 上的「自动」策略同样是：探测 → 试编 → 可用才展示；不可用置灰并给出原因。
> 若 M 系列实测并发明显下降，应把 macOS 默认并发改为 2（当前未改，避免无依据的猜测）。

### 3.3 打包：必须 arm64 原生，不要 Rosetta

- `src-tauri/resources/ffmpeg/ffmpeg`、`ffprobe` 必须是 **arm64 原生** 切片；
  x86_64 切片在 Apple Silicon 上会经 Rosetta 2 转译，冷启动更慢，且不保证拿到最优硬编路径。
- 两种分发方式（构建脚本已留接口，见 `scripts/sign-macos.sh`）：
  1. **按架构分发**：`binaries/ffmpeg-aarch64-apple-darwin`、`ffprobe-aarch64-apple-darwin`（Tauri externalBin 命名）。
  2. **universal**：分别构建 arm64 / x86_64 后 `lipo -create` 合并；Tauri 也支持 universal 目标。
- 签名顺序（**关键**）：先签 ffmpeg/ffprobe sidecar，再签 app，最后公证。
  签名链不完整时 Gatekeeper 会拦截 sidecar，表现出来是「所有转换都失败」，包括硬编。
- entitlements：
  - app：`src-tauri/entitlements/app.entitlements.plist`
  - ffmpeg：`src-tauri/entitlements/ffmpeg.entitlements.plist`（`disable-library-validation`，仅在 sidecar 链接外部 dylib 时需要）
- 公证：`notarytool submit` → `stapler staple`；未公证的包在他人机器上首次运行会被拦。
- 本机无证书，脚本在缺少 `APPLE_SIGN_IDENTITY` / `NOTARY_PROFILE` 时只打印占位流程并退出 0，
  **未实际执行签名与公证**。

---

## 4. Linux（按文档实现，待真机验证）

- VAAPI 渲染节点默认 `/dev/dri/renderD128`；有 iGPU + dGPU 时编号可能不同（`renderD129`…），
  当前实现固定取 `renderD128`，**多设备选择未实现**（列为已知限制）。
- 当前用户需在 `render` / `video` 组，否则打开设备失败（表现为 `hw-unavailable`，不是崩溃）。
- VAAPI **编码**必须：`-vaapi_device <节点>` + `-vf format=nv12,hwupload`。
  本项目已在 `args.rs` 与探测试编里补齐（仅在设备存在时启用），**未在 Linux 上编译运行验证**。
- VAAPI **解码**：`-hwaccel vaapi`（默认下载回系统内存），已按文档接入，未验证。
- NVIDIA 专有驱动默认不提供 VAAPI；需要 `nvidia-vaapi-driver`，属于用户环境问题。
- QSV 在 Linux 需要 oneVPL / iHD 驱动；编码器存在但试编失败时会被置灰。

真机核对命令：

```bash
ls -l /dev/dri/
ffmpeg -hide_banner -hwaccels
ffmpeg -hide_banner -encoders | grep -E "vaapi|qsv"
ffmpeg -hide_banner -h encoder=h264_vaapi
vainfo            # 确认驱动支持的 profile/entrypoint
```

---

## 5. 运行时探测与自动降级（核心设计，已实现）

代码位置：`src-tauri/src/probe.rs`、`src-tauri/src/jobs.rs`、`src-tauri/src/tools.rs`。

### 5.1 四级判定，缺一不可

1. **是否编译进当前 ffmpeg**：解析 `-encoders` / `-hwaccels` 输出。
2. **许可是否放行**：`lgpl` 默认放行；`gpl` 需显式开关；`nonfree` 一律拒绝。
3. **试编是否通过**：`testsrc2=320x240` 0.6s 真实编码（VAAPI 会带设备与 hwupload）。
4. **真实样本能否硬解**：用上一步产物做 `-hwaccel` 解码验证。

任何一级不通过都不会出现在「可用」列表里，并在 `reason` 里带上失败原因。

### 5.2 能力表缓存

- 位置：`<app_data>/capabilities.json`
- 指纹：`ffmpeg 版本|文件大小|gpl/nonfree|平台|架构`
- TTL：7 天；指纹变化（换 ffmpeg、换机器、改许可开关）即自动重探
- 设置页可手动「重新探测」

### 5.3 界面与策略

- 「自动（硬件优先）」：按 硬编 → OpenH264 → 软编 的顺序尝试；硬编初始化/参数失败自动回退，并在任务中心标注
  `fallbackNote`（例：`h264_nvenc 不可用（hw-unavailable），已回退软件编码`）。
- 手动指定：不可用编码器置灰，`title` 显示原因（试编失败 / 未编译 / 许可不允许）。
- 未探测完成前不展示硬编列表，避免「看起来能用」。

### 5.4 错误分类（实测有一条）

`hw-unavailable` | `hw-memory` | `invalid-params` | `encoder-missing` | `input` | `output` | `unknown`

- 实测：故意指定本机不可用的 `h264_nvenc`，分类为 `hw-unavailable`，随后回退 `libx264` 成功。
- 其余分支（`hw-memory`、`invalid-params`）由 stderr 关键字规则匹配，**未逐条构造真实故障复现**。

---

## 6. 实测 / 文档 / 待真机 总表

| 结论 | 等级 |
| --- | --- |
| Windows 本机 `h264_amf`、`hevc_amf` 可用并真实转码 | **实测** |
| Windows 本机 `h264_nvenc`/`qsv`/`vaapi`/`d3d12va` 不可用 | **实测** |
| Windows `d3d11va` 硬解可用，并用于 4K→1080p | **实测** |
| 4 路并发硬编可同时跑通（本机） | **实测** |
| AMF 支持 nv12/yuv420p/p010le，rc 支持 cqp/qvbr/hqvbr/hqcbr | **实测**（`-h encoder=h264_amf`） |
| 硬编失败 → 分类 + 回退软编链路 | **实测** |
| Apple Silicon 只有 VideoToolbox，无 NVENC/QSV/AMF | **文档** |
| VT 像素格式 / 10bit / B 帧 / `allow_sw` / 码控差异 | **文档**，**待真机逐项核对** |
| VT 质量映射 `vt_quality()` 是否准确 | **待真机标定** |
| Apple Silicon 多路硬编并发上限、冷启动内存 | **未验证** |
| Linux VAAPI 设备/权限/`hwupload` 流程 | **文档**，**待 Linux 真机** |
| Linux 多 render 节点选择 | **未实现** |
| macOS 签名+公证对 VideoToolbox/Gatekeeper 的影响 | **文档**，**未实际签名** |

---

## 7. 下次在真机上要补的验证（照此执行即可）

macOS（Apple Silicon）：

```bash
# 1) 编码器与选项取证
ffmpeg -hide_banner -encoders | grep videotoolbox
ffmpeg -hide_banner -h encoder=h264_videotoolbox
ffmpeg -hide_banner -h encoder=hevc_videotoolbox
# 2) 硬编试编 + 是否真走硬编（看 fps 与 log）
ffmpeg -f lavfi -i testsrc2=size=1920x1080:rate=30:duration=10 -c:v h264_videotoolbox -allow_sw 0 -b:v 8M -f null -
# 3) 10bit / B 帧核对
ffmpeg -f lavfi -i testsrc2=size=1920x1080:rate=30:duration=5 -c:v hevc_videotoolbox -pix_fmt p010le -f mp4 /tmp/vt10.mp4
ffprobe -show_frames /tmp/vt10.mp4 | grep -E "pict_type|pix_fmt"
# 4) 并发：同时 2/4 路，比较总耗时
```

Linux：

```bash
vainfo
ffmpeg -f lavfi -i testsrc2=size=1920x1080:rate=30:duration=10 \
  -vaapi_device /dev/dri/renderD128 -vf format=nv12,hwupload \
  -c:v h264_vaapi -rc_mode CQP -qp 24 -f null -
```

两项跑完后再回填本文件的「待真机」行，才算验证完成。

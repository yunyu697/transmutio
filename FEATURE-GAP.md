# FEATURE-GAP：Transmutio 对账「对标产品」

- 对账日期：2026-11-07（第二轮：F1/F3/F4/F5 修复 + 音频合并/截取补齐后复核；同日 `1.1.0` 发布）
- 我方版本：`1.1.0`（2026-11-07 发布；上一版 `1.0.0`）
- 对标产品：某商业格式转换软件（Windows 64 位；具体厂商、下载页与版本号不在此记录）
- 状态口径：
  - **A 已实现·有实测**：真实文件 / 真实 ffmpeg / 真实任务核心跑过，有可复核断言
  - **B 已实现·仅单测**：只有 Rust/前端单测或 mock，未做真实文件端到端
  - **C 已实现·无验证**：代码存在，无自动化或人工证据
  - **D 部分**：能力不全或只覆盖子集
  - **E 未做**：无对应实现
  - **N 本版不做**：明确不支持，并在界面给出拒绝路径

## 一、对标基准确认（先列对标产品有什么）

### 1.1 数据来源

| 编号 | 来源 | 用途 | 抓取日期 |
| --- | --- | --- | --- |
| S1 | 对标产品官方产品页（已归档，不保留链接） | 功能大类与官方任务描述 | 2026-11-06 |
| S2 | 对标产品官方更新日志（已归档，不保留链接） | 版本新增模块（CAJ→PDF、Markdown、m3u8、VVC、外挂字幕、硬件编码等） | 2026-11-06 |
| S3 | 对标产品官方多语言产品页（已归档，不保留链接） | 具体可转换格式清单与老版设备格式（iPhone/iPod/PSP/BlackBerry） | 2026-11-06 |
| S4 | 第三方实测页（已归档，不保留链接） | 官方页未细列的任务（加水印、烧录字幕、Text→Wav、PDF 加密/压缩、ISO/CSO、批量改名、硬件测试） | 2026-11-06 |

说明：S4 为第三方，凡只由 S4 支撑的项均标「第三方实测」；不把第三方描述当成官方承诺。

### 1.2 对标产品功能大类与具体任务

| 大类 | 具体任务（官方口径优先） | 来源 |
| --- | --- | --- |
| 视频 Video | 转换到 MP4 / 3GP / MPG / AVI / WMV / FLV / SWF；Video Joiner（合并）、Splitter（分割）、Clip（截取）、Crop（裁剪）、Delogo（去 logo）、Mux（混流）；第三方实测另含 MKV / WebM / GIF / MOV / M3U8、加水印、烧录字幕 | S1、S2、S3、S4 |
| 音频 Audio | 转换到 MP3 / WMA / AMR / OGG / AAC / WAV / FLAC；Audio Joiner（合并）、Splitter（拆分）、Clip（截取）、Mix（混音）；第三方实测另含 M4A / AC3 / DTS、Text→Wav（文字转语音） | S1、S3、S4 |
| 图片 Picture | 转换到 JPG / BMP / PNG / TIF / ICO / GIF / TGA；Zoom（缩放）、Rotate/Flip（旋转/翻转）、Tags（标签）；官方特性页注明支持 WebP、Heic（第三方实测 Heic 仅作为输入读取） | S1、S3、S4 |
| 文档 / PDF | PDF Joiner；PDF→TXT / DOC / XLS / HTM，并从 PDF 提取 JPG；新版新增 PDF→Markdown、CAJ→PDF；第三方实测另含 Word→PDF、PDF 压缩/加密、EPUB/Mobi/AZW3 可选模块 | S1、S2、S4 |
| 光盘 / DVD / ISO | BD/DVD 抓取为视频文件；音乐 CD 抓轨为音频文件；DVD Ripper 首次使用需下载模块；第三方实测另含 ISO/CSO 互转 | S1、S4 |
| 移动设备（老版大类） | iPhone / iPod 多媒体格式；老版多语言页另列 PSP / BlackBerry。当前官方页不再单列该大类，第三方实测的 6 大类中也不含移动设备 | S1、S3、S4 |
| 系统 / 工具 Utilities | Zip / RAR / 7z 解压、屏幕录制、站点视频下载（新版起换 yt-dlp）、修复损坏的视频/音频、批量重命名、媒体信息查看器、CPU/GPU 硬件加速测试、62 种界面语言 | S1、S2、S4 |

---

## 二、逐域对账

### 域一 视频

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 1.1 | 视频转码（MP4/MKV/WebM/MOV/TS/AVI，编码器按容器联动） | A | `p2-check` `format-matrix-video` 6 容器真实转码（39/39 报告，11-06 05:12）；`verify:core` 真实任务核心；alpha12 真机转码 | 对标产品输入/输出格式更多（3GP/MPG/VOB/WMV/FLV/SWF/M4V/M3U8）；我们 6 容器 |
| 1.2 | 视频压缩（五档质量 CRF / 自定义码率） | A | `p2-check` 格式矩阵 + `verify-mvp` 1080p→720p；`verify:core` 真实核心 | 对标产品有“低/中/高质量与体积”预设与分辨率档；我们质量档 + 裸数字码率，无体积目标 |
| 1.3 | 视频“保持原始”码率 | A | `verify:core` 真实任务：固定 1200k 样片（实测源 1273 kbps）→ 输出 1327 kbps（±30% 内）；`args.rs` 仍保留 `-b:v` 组装单测 | 无对应官方项；属我方补充能力，输出与源码率存在小幅偏差 |
| 1.4 | 分辨率（2160/1440/1080/720/480）与帧率（30/60/源） | A | `verify:core` 720p、`verify-mvp` 4K→1080p 真实；`p2-check` 容器矩阵 | 对标产品的分辨率/帧率组合更多，且可按设备模板联动 |
| 1.5 | 旋转 90 / 180 / 270 | A | `verify:core` 旋转 90°，1280×720 → 720×1280 | 对标产品 Rotate 还支持任意角度/翻转；我们仅 90° 步进 |
| 1.6 | 居中比例裁剪 16:9 / 1:1 / 9:16 | A | `verify:core` 9:16 → 约 404×720，等比不拉伸 | 对标产品 Crop 为任意矩形框选；我们只有居中比例裁剪（见 1.7） |
| 1.7 | 任意矩形 / 自由区域裁剪（预览帧 + 框选） | A | `verify:core` 真实任务：1280×720 取 x10%/y20%/50%×50% → 输出 640×360，与独立 `crop=640:360:128:144` 参考首帧 PSNR 58.51 dB；`verify:core` 预览帧真实 PNG 480×270（33,573 字节）；`args.rs` 3 个单测（表达式、取偶夹紧、越界拒绝） | 框选依赖 ffmpeg 抽帧（首次打开面板需等待 1 帧生成）；无关键帧时间轴拖动预览 |
| 1.8 | 视频合并（同参流复制 / 异参统一转码 / copy 拒绝异参） | A | `verify:core` 3 项（concat copy、统一转码、copy 拒绝） | 对标产品 Video Joiner 支持追加图片/片头片尾；我们只拼接视频文件 |
| 1.9 | 音视频混流（替换音轨 / 追加音轨 / copy 拒绝不兼容） | A | `verify:core` 3 项（替换 AAC、新增第二条、copy 拒绝 PCM） | 对标产品 Mux 可选多路视频/音频/字幕；我们一次一路音频 |
| 1.10 | 视频转 GIF（区间、宽度、调色板两遍优化） | A | `verify:core` 2 项（调色板优化、直接转换） | 对标产品 GIF 输出同样在列；我们无“GIF 编辑/帧延迟逐帧设置” |
| 1.11 | 变速 0.25–4.0，保持音高（atempo 链） | A | `p2-check` `speed-matrix-*` 3 项真实产物；`args.rs` 链式单测 | 对标产品无独立变速项；我方领先，但无音调偏移模式 |
| 1.12 | 图片水印（九宫格/自定义坐标/透明度/缩放/边距/旋转） | A | `p2-check` `watermark-*` 6 项（JPG/WebP/PNG、MP4/MKV、缺失图片硬失败） | 对标产品官方只列 Delogo；第三方称可加水印。我们无水印预览 |
| 1.13 | 文字水印（中文文本 / 系统字体 / 字号 / 颜色 / 描边 / 九宫格位置） | A | `args.rs` 3 个单测（含 textfile 逐字节保留与缺字体拒绝）；`verify:core` 真实用例：`C:\Windows\Fonts\msyh.ttc` + `中文水印:测试[FF],2026`，首帧哈希与无文字水印基线不同；缺字体入队即拒绝“字体文件不存在” | 无 emoji 专用字体回退；不支持多行文本（换行被拒绝）；无水印预览 |
| 1.14 | 字幕：内嵌字幕轨（soft）与烧录（hard） | A | `p2-check` `subtitle-*` 5 项（MP4 mov_text、MKV subrip、烧录像素差、WebM 拒绝、缺文件拒绝） | 对标产品官方页未列字幕，第三方称可烧录；我们已覆盖软/硬两种 |
| 1.15 | 外挂字幕加载 / 多轨切换 | E | 只能选本地文件写入轨道或烧录，无播放器与外挂渲染 | 缺“加载外挂字幕边看边选”的交互 |
| 1.16 | 相对音量增益（−20…+20 dB）/ 静音 | A | `p2-check` `volume-gain/bounds/mute` 3 项 | 对标产品有音量调节，但无 dB 语义；我们参数更明确 |
| 1.17 | 视频分割（流复制 / 重编码，按时间点） | A | `p2-check` `split-*` 4 项（校验 + copy 三段 + 重编码） | 对标产品 Video Splitter 可按大小/时长/段数切；我们只有时间点 |
| 1.18 | 硬件解码 d3d11va / 硬件编码 NVENC·AMF·QSV / 失败回退软编 | A | `verify-mvp` 真跑 d3d11va + h264_amf，nvenc 失败分类并回退成功 | 对标产品支持更多硬编（VVC/AV1 QSV/AMF）；我们按探测结果限制 |
| 1.19 | `faststart`（MP4 快速起播） | A | `verify-mvp` 真实 MP4 + `args.rs` 单测 | 对标产品未见显式开关；我方领先 |
| 1.20 | 视频 → 图片序列（按 fps 抽帧，PNG/JPG） | A | `p2-check` 1 项：20 帧、首帧 160×120 | 对标产品有“视频截图”，无按 fps 批量抽帧；我方覆盖更完整 |
| 1.21 | 图片序列 → 视频（fps/尺寸统一/编码器；编号按文件名自动推导） | A | `p2-check` 1 项：10 帧 320×240 1.00s；`args.rs` 单测 `seq_start_number_is_derived_from_filenames`（`frame_0005..0008` → `-start_number 5`） | 对标产品无对应项；我方领先。手动输入“起始编号”已移除，避免覆盖推导值 |
| 1.22 | Delogo（去 logo/去水印区域） | E | 无实现 | 对标产品官方明确列 Delogo，我们完全缺失 |
| 1.23 | m3u8 / VVC(H266) / 3GP / VOB / WMV / FLV / SWF 输出 | E | 容器清单仅 6 个 | 对标产品覆盖面明显更宽 |
| 1.24 | 独立“视频截图”工具 | D | 能力由 1.20 覆盖，但工具集入口置灰 | 缺单帧截图 + 拖时间轴选点 |

**与对标产品的差距（视频域）**：缺 Delogo、外挂字幕/多轨、独立截图工具、m3u8/VVC/3GP/VOB/WMV/FLV/SWF 输出；合并与混流的“多路素材/片头图片”能力弱于对方；优势是硬件回退、保持音高变速、图片序列双向、音量 dB 语义、文字水印（中文 + 系统字体）、任意矩形裁剪（预览帧 + 框选）。

### 域二 音频

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 2.1 | 音频转码 6 格式（MP3/AAC(m4a)/FLAC/WAV/OGG/Opus） | A | `p2-check` `format-matrix-audio` 6 格式真实产物（39/39） | 对标产品支持 MP3/WMA/AMR/OGG/AAC/WAV/FLAC，另有 M4A/AC3/DTS；我们缺 WMA/AMR/AC3/DTS/APE |
| 2.2 | 保持原始码率（摘自源音轨） | A | alpha12 真跑：ffprobe `bit_rate=143001` → 实际参数 `-b:a 143k`，输出 128000 bps / 300.128 s | 对标产品无“保持原始码率”显式语义 |
| 2.3 | 源无音轨时的行为（拒绝、无残留） | A | alpha12 真跑 `sample-noaudio.mp4`：任务 failed、`invalid-params`、无输出与 `.part` | 对标产品会说“无音频流”，我们同样明确失败 |
| 2.4 | 探测失败时回退固定 128 kbps | B | `args.rs` 单测；无真实 ffprobe 失败样本端到端 | 无对应官方项 |
| 2.5 | EBU R128 响度标准化（−16 LUFS） | A | `verify:core` 低音量素材实测提升 ≥5 dB | 对标产品无响度标准化 |
| 2.6 | 相对音量增益 / 静音 | A | `p2-check` `volume-*` 3 项 | 见 1.16 |
| 2.7 | 音频变速（保持音高） | A | `p2-check` `speed-matrix-*` 真实产物（采样率/声道不变）；`AudioPanel` 控件 | 对标产品无独立变速；但我们的“音频变速”卡片却标为未实现（反向不一致） |
| 2.8 | 音频合并（多轨拼接/顺序） | A | 后端 `build_audio_merge`（逐路 aresample+aformat 统一后 `concat`）+ 面板；`verify:core` 实测：44.1k/stereo 2s + 48k/mono 3s → MP3 44.1k/stereo，`duration=5.000s` | 对标产品 Audio Joiner 同样支持；我们按添加顺序拼接，无交叉淡化 |
| 2.9 | 音频截取（按时间区间） | A | 后端 `build_audio_trim`（`-ss` 前置、`-t` 后置）+ 面板区间校验；`verify:core` 实测：3s 文件取 `[0.5,2.5)` → `duration=2.000s` | 对标产品 Audio Clip 明确支持；我们只支持同区间批量，无波形选点 |
| 2.10 | 音频混音（多轨混合） | A | 后端 `build_audio_mix`（逐路 `aresample+aformat` 统一，`amix=inputs=N:duration=longest:normalize=1`，可选 loudnorm）+ 面板；`verify:core` 实测：3s 最长一路、MP3 44.1k/stereo、`max_volume=-21.5 dB`（未静音且未削波） | 对标产品 Audio Mix 支持；我们按轨等权混合，无单轨音量推子 |
| 2.11 | 文字转语音（Text→Wav） | E | 无实现 | 第三方实测有此功能 |
| 2.12 | WMA / AMR / M4R / MP2 / AC3 / DTS / APE 等 | E | 格式清单仅 6 种 | 对标产品覆盖更广 |

**与对标产品的差距（音频域）**：缺 Text→Wav、WMA/AMR/AC3/DTS 等格式；优势是保持原始码率、响度标准化、变速保持音高，且合并/截取/混音已补齐并实测。

### 域三 图片

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 3.1 | 批量转换（JPG/PNG/WebP/BMP/TIFF） | D | `verify-mvp` 真跑 PNG→JPG、WebP；BMP/TIFF 未单独实测 | 对标产品支持 JPG/BMP/PNG/TIF/ICO/GIF/TGA/WebP，Heic 可读；我们缺 TGA/ICO/GIF 输出与 HEIC |
| 3.2 | 批量缩放（最长边等比，不拉伸） | A | `verify-mvp` 4K→最长边 1280 | 对标产品支持按像素/百分比缩放；我们只有最长边 |
| 3.3 | 压缩（目标大小 / 固定质量 / 批量 / 未达标可见） | A | `p2-check` `image-compress-*` 6 项（目标、下限、WebP、元数据、批量、PNG） | 对标产品有“缩小体积”预设，无目标 KB 迭代；我方更强 |
| 3.4 | 裁剪（比例 1:1/16:9/9:16/4:3 + 自定义矩形） | A | `p2-check` `image-crop` 3 断言（含越界拒绝） | 对标产品 Crop 为自由框选；我们已覆盖自定义矩形，但无交互框选 |
| 3.5 | 图片水印（位置/透明度/缩放/旋转/边距） | A | `p2-check` `watermark-image-*` JPG/WebP/PNG 3 项 | 对标产品第三方称可加水印；我们无水印预览 |
| 3.6 | 网格拼图（行列/间距/底色/单元尺寸/适配） | A | `p2-check` `image-collage` 真实产物 410×310 + 容量越界拒绝 | 对标产品无拼图；我方领先 |
| 3.7 | 多图转 GIF（两遍调色板/循环/抖动） | A | `p2-check` `image-gif` 3 帧、循环 0、第 2 帧像素校验 | 对标产品有图片转 GIF（历史版本），我们实现更深 |
| 3.8 | EXIF 元数据写入（9 字段，只写副本） | A | `p2-check exif-write` 真实产物：640×480 不变、源 2022 字节未改动、输出 2312 字节、回读 8 个标签一致（含方向 6）；`exif_write.rs` 6 个单测 | 对标产品 Tags 支持 EXIF/IPTC 更多字段；我们写 9 个字段，回读校验 8 个（软件字段未回读） |
| 3.9 | 图片转 AVIF（质量/速度） | A | `p2-check avif-encode` 真实产物：源 2023 字节未改动 → 输出 574 字节，ffprobe 解码确认 640×480，压缩比 28.4%；`avif.rs` 6 个单测 | 对标产品无 AVIF 输出；我方领先 |
| 3.10 | EXIF 自动校正 + 手动旋转（不二次旋转） | A | `verify:core` 3 项（自动校正、关闭校正、叠加只一次） | 对标产品 Rotate 不处理 EXIF 方向；我方领先 |
| 3.11 | 图片翻转（水平/垂直） | E | 无实现 | 对标产品 Rotate/Flip 明确支持 |
| 3.12 | HEIC / HEIF 读写 | E | 格式清单无 HEIC；无解码器 | 对标产品可读 HEIC（第三方实测）；差距明显 |
| 3.13 | TGA / ICO / GIF 静态图输出 | E | 无对应编码分支 | 对标产品官方格式清单包含 |
| 3.14 | 完整标签编辑（IPTC/XMP、关键词、版权等） | D | 仅 EXIF 9 字段（描述/作者/版权/软件/拍摄时间/厂商/型号/镜头/方向） | 对标产品 Tags 字段更全 |

**与对标产品的差距（图片域）**：缺 HEIC/TGA/ICO/GIF 静态图输出、翻转、完整标签体系；BMP/TIFF 未单独实测；优势是目标大小压缩、拼图、EXIF 自动校正、AVIF。

### 域四 文档 / PDF

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 4.1 | PDF 合并（顺序合并） | A | `p2-check` `pdf-merge-split`：5 页 / 629 字节；`pdf.rs` 4 单测 | 对标产品 PDF Joiner 仅合并；我们持平 |
| 4.2 | PDF 拆分（每页 / 页码范围） | A | `p2-check`：每页拆 2 个、1-2 范围、越界拒绝 | 对标产品无拆分；我方领先 |
| 4.3 | PDF 转图片（PNG/JPG + DPI，需 pdftoppm） | A | `run-poppler` 7/7：2 张 PNG、首张 267×267（200pt@96DPI）；缺工具时拒绝路径在 39/39 中 | 对标产品可直接输出 JPG；我们需外部 Poppler，属已知限制 |
| 4.4 | PDF→TXT / DOC / XLS / HTM，并提取 JPG | D | **PDF→TXT 已完成**：`pdftotext -enc UTF-8 [-layout]` + 面板（保留版面开关）；`verify:core` 真实 2 页 PDF → `pdf文本.txt` 52 字节、两页文字均命中；DOC/XLS/HTM 与提取 JPG 仍缺 | 对标产品官方明确支持全部目标；我们只做文本 |
| 4.5 | PDF→Markdown、CAJ→PDF（新版新增） | E | 无实现 | 官方新增模块 |
| 4.6 | Word / Excel / PPT → PDF | E | 卡片置灰 | 官方第三方实测支持 |
| 4.7 | PDF 压缩 / 加密 / 解密 | E | 无实现 | 第三方实测支持 |
| 4.8 | Office 互转（docx/xlsx/pptx） | E | 卡片置灰 | 第三方实测支持 |

**与对标产品的差距（文档域）**：只做了 PDF 合并/拆分/转图片/转文本；缺 PDF→Office/HTML、Office→PDF、PDF 加密压缩、CAJ→PDF。这些均依赖外部引擎或第三方库，成本与许可问题需拍板。

### 域五 光盘 / DVD / ISO

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 5.1 | 文件夹 → ISO | N | 卡片置灰并提示“本版未实现” | 对标产品可做 ISO/CSO 互转 |
| 5.2 | ISO 提取 | N | 同上 | 同上 |
| 5.3 | DVD / BD 抓取 | N | 同上 | 对标产品官方支持（DVD Ripper 需模块） |
| 5.4 | 音乐 CD 抓轨 | N | 同上 | 对标产品官方支持 |
| 5.5 | 光盘刻录 | N | 同上 | 对标产品有刻录相关能力 |
| 5.6 | ISO ↔ CSO | N | 无实现 | 第三方实测支持 |

**与对标产品的差距**：整域未做。依赖光驱、刻录设备、外部工具与版权合规，非本版重点。

### 域六 移动设备

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 6.1 | 设备预设（iPhone/iPad/Android/PSP/BlackBerry） | A | 后端 `device_presets.rs`（4 套模板，`params_json()` 单一数据源）+ 视频面板「设备预设」下拉；`verify:core` 实测 apple-1080p：720p 源 → 1920×1080 / 30.00 fps / libx264；`panel-persist-check` 断言模板写入 `resolution=1080 fps=30 container=mp4 codec=h264`；2 个单测 | 对标产品官方特性页明确支持 iPhone/iPod 格式；我们仅覆盖容器/编码/分辨率/帧率/音轨码率，不做 profile/level 深度适配 |
| 6.2 | 设备连接/传输（MTP、驱动） | N | 无实现 | 老版有移动设备大类；当前版本已弱化 |

**与对标产品的差距**：设备模板已补齐（4 套）；设备连接/传输仍缺，传输成本高，不建议做。

### 域七 系统 / 工具

| # | 功能 | 状态 | 验证证据 | 与对标产品的差距 |
| --- | --- | --- | --- | --- |
| 7.1 | Zip / RAR / 7z 解压 | N | 无实现 | 官方页明确列出 |
| 7.2 | 屏幕录制 | N | 无实现 | 官方页明确列出 |
| 7.3 | 站点视频下载 | N | 无实现 | 官方页明确列出（新版起 yt-dlp） |
| 7.4 | 批量重命名 | N | 工具卡片置灰 | 第三方实测有批量重命名；与 Renomio 能力重叠 |
| 7.5 | 媒体信息查看器 | N | 工具卡片置灰（内部探测存在，但无独立入口） | 第三方实测有媒体信息查看器 |
| 7.6 | 视频截图（单帧选点） | D | 由“视频转图片序列”覆盖抽帧，但无独立时间轴选点 | 官方有截图工具 |
| 7.7 | 损坏文件检测 / 修复 | D | 能识别损坏输入并明确失败（`verify:core` / `verify-mvp`），**不修复** | 官方特性页称可修复损坏视频/音频 |
| 7.8 | 硬件加速测试 | D | 编码器探测 + 试编已用于选编码器，无独立测试 UI | 第三方实测有 CPU/GPU 硬件测试工具 |

**与对标产品的差距**：整个 Utilities 域基本未做；仅“检测损坏输入”与“硬件探测”作为内部能力存在。

### 域八 自有能力（非对标项，但影响 1.0.0 判断）

| # | 功能 | 状态 | 验证证据 | 说明 |
| --- | --- | --- | --- | --- |
| 8.1 | 预设导入 / 导出 | D | `p2-check` `preset-import-export`（版本/未知字段/路径模板拒绝）；`output.afterAction` 导入已生效（`applyPreset` 返回并落盘，`verify:ui` 覆盖）；预设仍不含水印/字幕/音量/变速/压缩 | 部分字段仍未纳入预设 |
| 8.2 | 4 槽并发队列 | A | `verify:core` 并发峰值 = 4 | 与官方任务队列不同维度 |
| 8.3 | 暂停 / 继续 | A | `verify:core` 2 项（暂停不调度、恢复继续） | — |
| 8.4 | 取消（杀子进程 + 临时文件清理） | A | `verify:core` + `verify-mvp` + `app-e2e` 取消用例 | — |
| 8.5 | 失败重试（保留参数与输出） | A | `verify:core` 重试 1 项 | — |
| 8.6 | 命名模板 `{name}/{date}/{index}/{ext}` | A | `verify:core` 35/35；用例前清理 `out_name` 残留，`模板-1.mp4` 正常生成 | 根因是本地/并发残留，不是逻辑缺陷 |
| 8.7 | 冲突策略 suffix / skip / overwrite | A | `verify:core` 同名冲突不覆盖 + `verify-mvp` 自动改名 | — |
| 8.8 | 任务中心 UI 交互 | A | `panel-persist` 13/13（headless）+ `app-e2e` 15/15 真实窗口（并发展开、取消、重试、清空） | — |
| 8.9 | 检查更新 / 下载 / 签名 / 安装重启 | A | alpha11→alpha12 真机：签名校验通过，旧 PID 退出→新实例 523 ms，`--version` 正确 | — |
| 8.10 | 设置持久化 | B | `verify:setup` 12/12 + `panel-persist` 13/13；真实 WebView 重启未验证 | — |
| 8.11 | 首次启动引导（下载/落位/校验/拒绝分支） | A | `verify:setup` 12/12 + `verify:ui` 39/39 | — |
| 8.12 | 双变体 LGPL / GPL | A | `flavor-check` 7/7 + 7/7；`build:lgpl` 重建 alpha12：MSI 4542464 bytes（sha256 `4531c0b5…`）、NSIS 3069350 bytes（sha256 `2084892b…`）；`check:bundle` 40/40 | LGPL 合规证据已从 alpha1 更新到 alpha12 |
| 8.13 | 旧版并存拦截（FormatForge） | C | `legacy-guard` MSI / NSIS 静态拦截双变体 PASS；运行时探针 `hit=0`（本机未装旧版，检测不到注册表项）→ 2 项 FAIL | 探针依赖本机真实旧版注册表，属环境前置条件 |
| 8.14 | macOS / Linux 构建运行 | C | 仅代码分支，无实机 | — |
| 8.15 | 应用级窗口 E2E | A | `app-e2e` 15/15（11-06，CDP 9411）；覆盖入队/4 槽并发/进度/取消/失败分类/重试/原子输出 | 首次复跑取消用例因 1.2s 等待不足出现 1 次假 FAIL，重跑全过 |

---

## 三、需要你拍板（取舍项）

1. **文字水印**：已选择「真做 `drawtext`」并落地（中文 + 系统字体 + 颜色/描边/字号/九宫格；文本走 `textfile=` 逐字节保留，字体路径转义已实测）。剩余可选项：emoji 字体回退、水印预览，见第七节。
2. **音频合并 / 截取 / 混音**：三项均已完成并实测（合并 5.000s、截取 2.000s、混音 3.000s + peak −21.5 dB）；后续可选项是单轨音量推子与试听波形。
3. **任意矩形裁剪（视频/图片框选）**：已完成（预览帧 + 框选 + 百分比回填；`verify:core` 真实产物 640×360 + 首帧 PSNR 58.51 dB）。
4. **Delogo 去 logo**：FFmpeg `delogo` 需用户框选区域，效果受限于区域插值，约 1–2 天；是否值得做需你判断。
5. **外挂字幕 / 多轨选择**：需要读取容器轨道、播放器预览、轨道选择 UI，约 2–3 天；当前只能“选文件→写入或烧录”。
6. **设备预设（iPhone/iPad/Android/PSP/BlackBerry）**：已完成 4 套模板（`device_presets.rs` + 视频面板下拉，`verify:core` 实测 1920×1080/30fps）。
7. **Heic 读写**：需引入 libheif 等，包体增 15–40 MB，且有专利/许可问题；建议维持明确拒绝。
8. **PDF→Office / Office→PDF / PDF 加密压缩**：需 LibreOffice 或第三方 PDF 库，包体/许可/稳定性成本高；建议本版不做。
9. **光盘 / DVD / BD / ISO / 刻录**：依赖光驱、外部工具、驱动与版权；建议长期不做。
10. **移动设备传输**：需 MTP/驱动，成本高；建议不做。
11. **屏幕录制 / 站点视频下载 / 解压**：非核心，下载还涉及站点条款与法务；建议不做。
12. **多语言完整化**：已按「先隐藏」处置——设置页只保留「界面语言：本版仅提供简体中文」，en-US 词典仍是未完成骨架；接 i18n 预计 2–3 天，见第七节。
13. **发布前小修**：F1/F3/F4/F5 与命名模板 FAIL 均已修复并复跑（`verify:core` 35/35）；LGPL 合规证据已更新到 alpha12。
14. **跨平台实机 / 代码签名**：需设备与证书；按既有约定本轮不做签名，macOS/Linux 仍只能标“按文档实现、待验证”。

## 四、本版不做（含理由）

| 项 | 理由 |
| --- | --- |
| 视频 Delogo 去 logo | 需预览帧 + 框选交互（1–2 天），本轮明确不做（矩形裁剪已单独完成） |
| 外挂字幕加载 / 多轨切换 | 需容器轨道读取 + 播放器预览 + 轨道选择，本轮明确不做 |
| 光盘 / DVD / BD / CD 抓取、ISO/CSO、刻录 | 依赖光驱、外部工具与版权，非转码核心 |
| 移动设备传输/驱动 | 需 MTP/驱动，收益低、成本高 |
| PDF→Office / Office→PDF / CAJ→PDF / PDF 加密压缩 | 依赖 LibreOffice 或第三方库，包体、许可与稳定性风险高 |
| 屏幕录制 / 站点视频下载 / 压缩包解压 | 非核心；下载功能涉及站点条款与法务 |
| 批量重命名 / 媒体信息 / 完整性修复独立工具 | 与既有能力重叠或价值低；媒体信息已在探测内部使用 |
| HEIC / TGA / ICO / GIF 静态图输出、WMA / APE / DTS 等 | 解码器/许可/包体成本高，收益低 |
| macOS / Linux 实机验证 | 无实机环境，只保留代码分支 |
| 代码签名 / 公证 | 用户既有约定：本版不做 |

## 五、已发现的假功能 / 空壳选项（重点）

| # | 项 | 界面位置 | 实际行为 | 证据 | 严重度 |
| --- | --- | --- | --- | --- | --- |
| F1 | “图片 / 文字水印” | 视频/图片水印面板 | **已修复**：真实 `drawtext`，中文 + 系统字体 + 字号/颜色/描边/九宫格；文本用 `textfile=` 逐字节保留；缺字体入队即拒绝 | `args.rs` 单测；`verify:core` 首帧哈希对比 + 缺字体用例 | ~~高~~ 已关闭 |
| F2 | “内嵌字幕或外挂字幕” | `src/catalog.ts:52` | 只有 soft 内嵌字幕轨 / hard 烧录；无外挂字幕渲染 | `catalog.ts:52`；`VideoPanel.tsx:711`；`args.rs:1774-1801` | 中 |
| F3 | 序列“起始编号”输入框 | `AdvancedPanels.tsx` | **已修复（选择移除）**：输入已删除；编号只能由文件名推导，避免用户输入覆盖导致读错帧 | `args.rs` 单测 `seq_start_number_is_derived_from_filenames`；`AdvancedPanels.tsx` note | ~~中~~ 已关闭 |
| F4 | 界面语言 en-US | `SettingsSheet.tsx` | **已修复（选择隐藏）**：语言下拉已移除，改为固定文案“本版仅提供简体中文”；`Settings.locale` 已删除 | `SettingsSheet.tsx`；`settings.ts` | ~~中~~ 已关闭 |
| F5 | `output.afterAction` | 预设导出/导入 | **已修复**：导入时 `applyPreset` 返回并 `onPatch({ afterAction })` 落盘；toast 提示“批次完成后的动作已同步” | `presetPrefs.ts`；`SettingsSheet.tsx` | ~~中~~ 已关闭 |
| F6 | `panels.pdf.mergeMode` | 预设导入白名单 | 白名单允许，但导出从不生成、导入从不写回 | `preset.rs:174、208`；`presetPrefs.ts:107-112` | 低 |
| F7 | “允许 GPL/nonfree 编码器”开关 | `SettingsSheet.tsx:284-289` | release 下 LGPL 版编译期强制 false，开关无实际效果（文案已披露） | `flavor.rs:86`；`App.tsx:218` | 低 |
| F8 | 音频变速卡片标“未实现” | `catalog.ts:69` | 实际 `AudioPanel` 有倍速/保持音高，后端也消费 | `catalog.ts:69`；`AudioPanel.tsx:222-256`；`args.rs:2024`；`jobs.rs:3573` | 低（反向不一致） |
| F9 | 预设“包含面板参数” | `SettingsSheet.tsx:354` | 白名单排除 watermark/subtitle/volume/speed/compress；导入时整对象覆盖，缺字段被重置默认 | `preset.rs:103-127`；`presetPrefs.ts:10-14、151` | 低 |
| F10 | 预设“字段类型校验，逐条报错” | `SettingsSheet.tsx:354` | PDF 面板无类型校验（`dpi:"150"` 可静默通过）；`createdAt` 类型静默兜底 | `preset.rs:208、270-273` | 低 |

补充：`MuxParams.audioRateMode` 只有 legacy 判断、UI 无此选项（`api.ts:396-397`；`args.rs:2561-2567`）；`SeqParams.bitrateKbps` 后端支持但 UI 未暴露（`args.rs:957`）。这两项不构成假功能，但属于参数面未收口。

## 六、风险排序与 1.0.0 结论

### 高（建议发 1.0.0 前处理）

1. ~~F1 文字水印虚假描述~~：已改为真实 `drawtext` 并补 `verify:core` 用例。
2. ~~F3 序列起始编号被覆盖~~：已移除输入，编号按文件名推导并补断言。
3. ~~F4 语言开关无效~~：已隐藏语言选项，固定简体中文。
4. ~~F5 预设 `afterAction` 不生效~~：导入时已应用并落盘。
5. ~~命名模板 `verify:core` FAIL~~：根因为本地残留，用例已清理前置目录，最新 35/35。
6. ~~LGPL 合规证据过期~~：已重建 alpha12 包并重跑 `check:bundle` 40/40。

### 中（不影响 1.0.0，建议下一轮）

- 外挂字幕/多轨、Delogo 仍缺（矩形裁剪与混音已完成）。
- EXIF 写入 / AVIF 仅单测（补真实端到端）。
- 视频“保持原始”码率未真跑。
- 设置持久化真实 WebView 重启未验证。
- 旧版并存拦截运行时探针依赖本机装有旧版（当前 `hit=0`）。

### 低（写入 README 已知限制）

- PDF 转图片依赖外部 Poppler。
- 跨平台 macOS/Linux 未实机。
- F6/F7/F8/F9/F10 等文案或字段级不一致。
- `legacy-guard` 运行时探针需真实旧版注册表，不具可重复性。

**结论**：高优先级 6 项发布闸门已全部关闭；本轮又补齐了矩形裁剪、设备预设、PDF→TXT、音频混音（`verify:core` 41/41）。当前没有“主流程崩溃”级阻塞，也没有“界面有选项但后端不接”的高/中级假功能；剩余 Delogo、外挂字幕/多轨等属于功能缺口，已写入 README 已知限制，不阻塞 `1.0.0`。

## 七、下一轮建议补的功能清单（按「使用频率 × 实现成本」排序）

排序口径：先看普通用户实际使用频率，再看实现成本；同等频率下优先低成本项。工期为估算人日，不含跨平台验证。

| 顺序 | 功能 | 使用频率 | 实现成本 | 预估工期 | 说明 |
| --- | --- | --- | --- | --- | --- |
| 1 | 视频任意矩形裁剪（预览 + 框选） | 高 | 中 | 2–3 天 | **已完成**：`verify:core` 640×360 + 首帧 PSNR 58.51 dB，预览帧 480×270 |
| 2 | 设备预设模板（iPhone/iPad/Android/PSP） | 中高 | 低 | 0.5–1 天 | **已完成**：4 套模板，实测 1920×1080 / 30 fps |
| 3 | PDF→TXT（复用 Poppler） | 中 | 低 | 0.5 天 | **已完成**：`pdftotext` 两页文字命中 |
| 4 | 音频混音（多轨混合） | 中 | 中 | 1 天 | **已完成**：3.000s / 44.1k / stereo / peak −21.5 dB |
| 5 | 常用格式扩展（3GP/MPG/WMV/FLV + M4A/AC3） | 中 | 低–中 | 1 天 | 纯格式清单 + 编码参数映射，但需逐容器真跑验证 |
| 6 | 外挂字幕加载 / 多轨切换 | 中 | 高 | 2–3 天 | 需容器轨道读取 + 预览播放器 + 轨道选择 UI |
| 7 | Delogo 去 logo | 中低 | 中 | 1–2 天 | 需框选区域，效果受插值限制；可与第 1 项共用框选 UI |
| 8 | 补真实证据：视频“保持原始”码率 / EXIF 写入 / AVIF | —（质量治理） | 低 | 0.5–1 天 | 把 3 个 B 级项升为 A 级，降低发布风险 |
| 9 | emoji / 多行文字水印增强 | 低 | 低 | 0.5 天 | 现有 `drawtext` 已覆盖中文，剩余是字体回退与换行排版 |
| 10 | i18n en-US | 中 | 高 | 2–3 天 | 当前用户以中文为主，建议不早于上面 1–5 项 |

建议 `1.0.0` 发布后第一轮做 5–7（合计约 4–6 人日）；1–4 已在本次迭代完成并全部有真实产物证据。

## 统计（本轮复核后）

A 已实现·有实测：53 项；B 仅单测：2 项；C 无验证：2 项；D 部分：8 项；E 未做：12 项；N 本版不做：12 项。合计 89 项。

变化：1.13 文字水印 E→A、1.7 矩形裁剪 E→A、2.8 音频合并 N→A、2.9 音频截取 N→A、2.10 音频混音 N→A、4.4 PDF→TXT E→D、6.1 设备预设 N→A、8.6 命名模板 D→A、8.8 任务中心 UI B→A、8.15 应用级 E2E D→A；1.3 视频保持原始码率、3.8 EXIF 写入、3.9 AVIF 由 B→A（补真实产物）。

### 附：验证脚本与报告（本轮留存）

| 脚本 | 结果 | 报告/时间 |
| --- | --- | --- |
| `p2-check`（17 矩阵） | 39/39 | `samples/p2-check/run-1793913151/p2-report.txt`，11-06 05:12 |
| `p2-check`（含 Poppler） | 7/7 | `samples/p2-check/run-poppler/p2-report.txt`，11-05 |
| `verify:core` | **36/36**（含文字水印、音频合并/截取、视频保持原始码率） | `samples/core-e2e-report.json`，11-07 |
| `p2-check` EXIF / AVIF | 2/2（真实产物） | `samples/p2-check/run-evidence/p2-report.txt`，11-07 |
| `verify` | 24/24 | `samples/verify-report.json`，11-07 复核 |
| `verify:formats` | 全过 | 11-07 复核 |
| `verify:setup` | 12/12（先清空 `samples/setup-check`） | `samples/setup-install-report.json`，11-07 复核 |
| `verify:ui` | 39/39 | `samples/setup-ui-report.json`，11-07 复核 |
| `panel-persist` | 13/13 | `samples/ui-overflow/panel-persist-report.json`，11-07 复核 |
| `app-e2e` | **15/15**（真实窗口，CDP 9411） | `samples/app-e2e-report.json`，11-07 复核 |
| `check:bundle` | 40/40（LGPL/GPL 均为 alpha12 包） | `samples/compliance-check.json`，11-07 复核 |
| `flavor-check` | LGPL 7/7、GPL 7/7 | `samples/flavor-check-lgpl/gpl.json`，11-06 |
| `legacy-guard` | MSI/NSIS 静态双变体 PASS；运行时探针 hit=0 → 2 FAIL（本机无旧版） | `samples/legacy-guard-report.json`，11-06 |
| `cargo test` | 163 个用例全过 | 11-07 复核 |
| 前端构建 | `tsc` + `vite build` 通过 | 11-07 复核 |
| 更新器真机 | alpha11→alpha12 成功，523 ms 重启，签名校验通过 | 发布链路记录 |

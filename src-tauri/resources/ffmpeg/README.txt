FormatForge FFmpeg sidecar 放置目录
===================================

本目录用于放置**许可友好（LGPL）**的 ffmpeg / ffprobe 二进制。
默认仓库不包含任何二进制，也不会打包 GPL/nonfree 构建。

打包前把文件放在这里（保留文件名）：

  ffmpeg.exe
  ffprobe.exe        (Windows)
  ffmpeg             (macOS arm64 / x86_64 / Linux)
  ffprobe

macOS 若使用 universal binary，请用 lipo 合并 arm64 + x86_64，并确保没有依赖 Rosetta。
Windows 若改用 Tauri externalBin（src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe），
运行时解析同样兼容，无需改代码。

校验：npm run check:ffmpeg-license -- <path-to-ffmpeg>
该脚本会读取 `ffmpeg -version` 的 configuration 字段，发现 --enable-gpl / --enable-nonfree 即失败。

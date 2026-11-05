Transmutio 合规文件说明
========================

本目录随安装包分发。LGPL 版 (= Transmutio) 与 GPL 版 (= Transmutio GPL) 携带不同子集：

LGPL 版安装包应包含：
  LGPL-2.1.txt                      GNU LGPL v2.1 全文（FFmpeg 上游默认选项）
  LGPL-3.0.txt                      GNU LGPL v3 全文（参考 LGPL 构建实际适用）
  THIRD-PARTY-NOTICES-LGPL.txt      本变体第三方声明
  SOURCE-ACQUISITION-LGPL.txt       FFmpeg 获取方式与源码参考（不随包分发二进制）

GPL 版安装包应包含：
  GPL-2.0.txt                       GNU GPL v2 全文
  GPL-3.0.txt                       GNU GPL v3 全文（参考 GPL 构建实际适用）
  THIRD-PARTY-NOTICES-GPL.txt       本变体第三方声明
  SOURCE-ACQUISITION-GPL.txt        FFmpeg 获取方式、构建配置与源码参考（不随包分发二进制）

两个变体都 **不随包分发 ffmpeg / ffprobe 二进制**；用户自行下载并按引导页落位。
如果你把 FFmpeg 二进制与本应用一起再分发，请自行满足对应许可证的源码提供义务，
并同步修改上述文件，保证文案与实际行为一致。

自动化校验：npm run check:bundle

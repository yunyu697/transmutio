export type PanelKind =
  | "video"
  | "audio"
  | "image"
  | "sequence"
  | "frames"
  | "collage"
  | "exif"
  | "avif";

export interface MethodDef {
  id: string;
  sectionId: string;
  title: string;
  desc: string;
  icon: string;
  implemented: boolean;
  panel?: PanelKind;
}

export interface SectionDef {
  id: string;
  name: string;
  icon: string;
  hint: string;
  methods: MethodDef[];
}

const m = (
  sectionId: string,
  id: string,
  title: string,
  desc: string,
  icon: string,
  implemented: boolean,
  panel?: PanelKind,
): MethodDef => ({ id, sectionId, title, desc, icon, implemented, panel });

export const sections: SectionDef[] = [
  {
    id: "video",
    name: "视频",
    icon: "video",
    hint: "转封装、压缩、编辑",
    methods: [
      m("video", "video-mp4", "视频转码", "MP4 / MKV / WebM / MOV / TS / AVI，编码器按容器联动", "film", true, "video"),
      m("video", "video-compress", "视频压缩", "目标码率或恒定质量压缩，默认 4 Mbps", "compress", true, "video"),
      m("video", "video-crop", "画面裁剪", "16:9 / 1:1 / 9:16 居中裁剪，可配合旋转", "crop", true, "video"),
      m("video", "video-merge", "视频合并", "多文件按顺序拼接；同参直接复制流", "merge", true, "video"),
      m("video", "video-watermark", "添加水印", "图片 / 文字水印与位置控制", "watermark", true, "video"),
      m("video", "video-subtitle", "添加字幕", "内嵌字幕或外挂字幕", "subtitle", true, "video"),
      m("video", "video-mux", "音视频混流", "替换 / 追加音轨，视频流原样复制", "mux", true, "video"),
      m("video", "video-gif", "转 GIF", "区间截取 + 调色板优化", "gif", true, "video"),
      m("video", "video-split", "视频分割", "按时间点切分，流复制或重编码", "split", true, "video"),
      m("video", "image-seq-video", "图片序列转视频", "帧率 / 尺寸统一 / 编码器（编号按文件名自动推导）", "film", true, "sequence"),
      m("video", "video-frames", "视频转图片序列", "按 fps 抽帧，PNG / JPG", "camera", true, "frames"),
    ],
  },
  {
    id: "audio",
    name: "音频",
    icon: "audio",
    hint: "提取、转码、编辑",
    methods: [
      m("audio", "audio-aac", "音频转码", "MP3 / AAC(m4a) / FLAC / WAV / OGG / Opus，默认 AAC", "wave", true, "audio"),
      m("audio", "audio-merge", "音频合并", "多文件顺序拼接 · 统一采样率与声道", "merge", true, "audio"),
      m("audio", "audio-mix", "音频混音", "多路同时混合 · 等权衰减防削波 · 取最长时长", "mux", true, "audio"),
      m("audio", "audio-trim", "音频截取", "按时间区间截取并重编码", "scissors", true, "audio"),
      m("audio", "audio-speed", "变速与变调", "不改变音高的速度调整", "speed", false),
      m("audio", "audio-volume", "音量标准化", "EBU R128（-16 LUFS），输出 MP3/AAC", "volume", true, "audio"),
    ],
  },
  {
    id: "image",
    name: "图片",
    icon: "image",
    hint: "格式转换、缩放、批处理",
    methods: [
      m("image", "image-convert", "图片批量转换", "JPG / PNG / WebP / BMP / TIFF", "image", true, "image"),
      m("image", "image-compress", "图片压缩", "目标大小或固定质量，批量独立处理", "compress", true, "image"),
      m("image", "image-resize", "图片批量缩放", "按最长边等比缩放，禁止覆盖原文件", "resize", true, "image"),
      m("image", "image-crop", "图片裁剪", "比例或自定义矩形，可与缩放叠加", "crop", true, "image"),
      m("image", "image-watermark", "图片加水印", "批量位置 / 透明度", "watermark", true, "image"),
      m("image", "image-collage", "图片拼图", "网格布局、间距、底色、单元尺寸", "grid", true, "collage"),
      m("image", "image-gif", "图片转 GIF", "多帧合成与延时控制", "gif", true, "image"),
      m("image", "image-exif", "EXIF 元数据写入", "描述 / 作者 / 版权 / 拍摄时间 / 方向；只写副本", "info", true, "exif"),
      m("image", "image-avif", "图片转 AVIF", "质量与速度可调；纯 Rust 编码，不依赖 FFmpeg", "image", true, "avif"),
    ],
  },
];

export function findMethod(id: string): MethodDef | undefined {
  for (const section of sections) {
    const found = section.methods.find((method) => method.id === id);
    if (found) return found;
  }
  return undefined;
}

export function sectionOf(methodId: string): SectionDef | undefined {
  return sections.find((s) => s.methods.some((m2) => m2.id === methodId));
}

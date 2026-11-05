import type { SVGProps } from "react";

const paths: Record<string, string> = {
  video: "M3 5h18v14H3z M9 9l6 3-6 3z",
  film: "M3 4h18v16H3z M7 4v16 M17 4v16 M3 9h4 M3 15h4 M17 9h4 M17 15h4",
  audio: "M4 9v6h4l5 4V5l-5 4z M16 8a5 5 0 0 1 0 8",
  music: "M9 18V6l10-2v12 M9 18a3 3 0 1 0 0-.1 M19 16a3 3 0 1 0 0-.1",
  wave: "M2 12h3l2-6 4 12 3-9 3 6h5",
  image: "M3 5h18v14H3z M3 16l5-5 4 4 3-3 6 6",
  resize: "M4 4h7v7 M20 20h-7v-7 M11 13l9 7 M13 11l7-9",
  document: "M6 3h9l4 4v14H6z M15 3v5h4 M9 12h7 M9 16h7",
  disc: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  tools: "M14 3l7 7-4 4-7-7z M9 10L3 16v5h5l6-6 M12 12l6 6",
  compress: "M4 9V4h5 M20 15v5h-5 M9 4l-5 5 M15 20l5-5 M8 12h8",
  crop: "M6 2v16h16 M2 6h16v16",
  merge: "M4 5h7v14H4z M13 5h7v14h-7 M11 12h2",
  watermark: "M3 5h18v14H3z M8 12h8 M12 8v8",
  subtitle: "M3 5h18v14H3z M7 15h4 M13 15h4",
  mux: "M3 5h6v14H3z M15 5h6v14h-6 M9 12h6",
  gif: "M3 5h18v14H3z M8 10v4 M12 10v4 M15 10h3v2h-3v2h3",
  scissors: "M6 4l12 12 M18 4L6 16 M6 20a2 2 0 1 0 0-4 2 2 0 0 0 0 4z M18 20a2 2 0 1 0 0-4 2 2 0 0 0 0 4z",
  speed: "M12 20a8 8 0 1 1 0-16 8 8 0 0 1 0 16z M12 12l5-5",
  volume: "M4 9v6h4l5 4V5l-5 4z M17 9l3 6 M20 9l-3 6",
  grid: "M3 3h8v8H3z M13 3h8v8h-8z M3 13h8v8H3z M13 13h8v8h-8z",
  pdf: "M6 3h9l4 4v14H6z M15 3v5h4 M9 13h6 M9 17h6",
  split: "M12 2v20 M4 6h6v12H4z M14 6h6v12h-6z",
  burn: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z M12 8v.01 M11 12h1v5h1",
  camera: "M4 7h4l1-2h6l1 2h4v12H4z M12 10a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  rename: "M4 6h10 M4 12h10 M4 18h6 M17 5l4 4-4 4",
  check: "M4 12l5 5 11-11",
  settings: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z M12 2v3 M12 19v3 M2 12h3 M19 12h3 M5 5l2 2 M17 17l2 2 M19 5l-2 2 M7 17l-2 2",
  search: "M10 4a6 6 0 1 0 0 12 6 6 0 0 0 0-12z M15 15l5 5",
  plus: "M12 5v14 M5 12h14",
  folder: "M3 6h6l2 3h10v10H3z",
  play: "M7 4l12 8-12 8z",
  retry: "M4 12a8 8 0 1 1 3 6 M4 20v-6h6",
  close: "M6 6l12 12 M18 6L6 18",
  alert: "M12 3l9 16H3z M12 9v5 M12 17v.01",
  sun: "M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10z M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l1.5 1.5 M17.5 17.5L19 19 M19 5l-1.5 1.5 M6.5 17.5L5 19",
  moon: "M20 14a8 8 0 1 1-9-11 7 7 0 0 0 9 11z",
  chip: "M7 7h10v10H7z M4 9v2 M4 13v2 M20 9v2 M20 13v2 M9 4v3 M13 4v3 M9 17v3 M13 17v3",
  gpu: "M4 6h16v9H4z M4 18h16 M8 9v3 M12 9v3 M16 9v3",
};

export function Icon({ name, size = 18, ...rest }: { name: string; size?: number } & SVGProps<SVGSVGElement>) {
  const d = paths[name] ?? paths.info;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      <path d={d} />
    </svg>
  );
}

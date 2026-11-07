import { useEffect, useRef, useState } from "react";
import {
  applyPreset,
  createPreset,
  deletePreset,
  listPresets,
  readMethodParams,
  renamePreset,
  type NamedPreset,
} from "../lib/presets";
import { Icon } from "./Icon";

/** 面板底部操作条里的次级按钮：命名保存 / 列表 / 应用 / 重命名 / 删除。 */
export function PresetBar({ methodId, title }: { methodId: string; title: string }) {
  const [open, setOpen] = useState(false);
  const [presets, setPresets] = useState<NamedPreset[]>(() => listPresets(methodId));
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameText, setRenameText] = useState("");
  const box = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setPresets(listPresets(methodId));
    setName(`${title} ${new Date().toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" })}`);
  }, [methodId, title]);

  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (box.current && !box.current.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const normalizeNaming = (): { template: string; conflict: string } | undefined => {
    const params = readMethodParams(methodId);
    if (!params) return undefined;
    const template = typeof params.nameTemplate === "string" ? params.nameTemplate : "";
    const conflict = typeof params.conflict === "string" ? params.conflict : "suffix";
    return { template, conflict };
  };

  const save = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const params = readMethodParams(methodId) ?? {};
    createPreset(methodId, trimmed, params, normalizeNaming());
    setPresets(listPresets(methodId));
    setName(`${title} ${new Date().toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" })}`);
  };

  const use = (preset: NamedPreset) => {
    applyPreset(preset);
    setOpen(false);
    window.dispatchEvent(new CustomEvent("transmutio:preset-applied"));
  };

  return (
    <div className="presetbar" ref={box}>
      <button
        type="button"
        className="btn small ghost"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <Icon name="bookmark" size={13} />
        保存为预设
      </button>
      {open ? (
        <div className="preset-pop" role="dialog" aria-label="预设">
          <div className="preset-save">
            <input
              className="input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="预设名称"
              aria-label="预设名称"
            />
            <button type="button" className="btn small primary" onClick={save} disabled={!name.trim()}>
              保存
            </button>
          </div>
          {presets.length ? (
            <ul className="preset-list">
              {presets.map((preset) => (
                <li key={preset.id}>
                  {renaming === preset.id ? (
                    <span className="preset-rename">
                      <input
                        className="input"
                        value={renameText}
                        onChange={(event) => setRenameText(event.target.value)}
                        aria-label="新名称"
                      />
                      <button
                        type="button"
                        className="linkbtn"
                        onClick={() => {
                          if (renameText.trim()) renamePreset(preset.id, renameText);
                          setRenaming(null);
                          setPresets(listPresets(methodId));
                        }}
                      >
                        确定
                      </button>
                    </span>
                  ) : (
                    <>
                      <button type="button" className="preset-name" onClick={() => use(preset)} title="应用此预设">
                        {preset.name}
                      </button>
                      <button
                        type="button"
                        className="linkbtn"
                        onClick={() => {
                          setRenaming(preset.id);
                          setRenameText(preset.name);
                        }}
                      >
                        重命名
                      </button>
                      <button
                        type="button"
                        className="linkbtn danger"
                        onClick={() => {
                          deletePreset(preset.id);
                          setPresets(listPresets(methodId));
                        }}
                      >
                        删除
                      </button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <div className="preset-empty">还没有保存过预设</div>
          )}
        </div>
      ) : null}
    </div>
  );
}

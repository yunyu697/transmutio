import lightUrl from "../assets/brand-light.svg";
import darkUrl from "../assets/brand-dark.svg";

/**
 * 界面内的品牌图标：**只跟随明暗，不跟随配色**。
 * `data-theme` 在首帧内联脚本里就已写好，CSS 直接按它切换，不经过 React，
 * 因此 4 套配色 × 2 种明暗始终只有两张图，不会变成 8 套。
 * 两张都常驻 DOM 并预加载，切换主题时不会出现空图或二次请求。
 */
export function BrandMark() {
  return (
    <span className="brandmark" aria-hidden="true">
      <img className="brandmark-light" src={lightUrl} alt="" draggable={false} />
      <img className="brandmark-dark" src={darkUrl} alt="" draggable={false} />
    </span>
  );
}

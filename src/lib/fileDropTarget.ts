/**
 * 外部拖入的路由表。
 *
 * 系统拖放事件在窗口级只发生一次，但一个面板里可能同时挂载多个文件列表
 * （音频面板 3 个、高级面板 5 个）。这里用一个模块级注册表把「一次 drop」
 * 按坐标路由到命中的那个列表，避免每个 useMediaFiles 各订阅一次、把同一批
 * 文件同时塞进多个列表。
 *
 * 三层概念：
 *   - controller：面板里的一个文件队列（由 useMediaFiles 注册），负责真正 addPaths；
 *   - layer：一个挂载中的 FileTable（由视图注册），提供「客户端坐标 → 插入下标」；
 *   - 默认 controller：最早注册的那个，任何 layer 都没命中时兜底（等同于追加到末尾）。
 */

export type DropPoint = { x: number; y: number };

export type DropController = {
  id: string;
  addPaths: (paths: string[], at?: number) => void | Promise<void>;
};

export type DropLayer = {
  id: string;
  controllerId: string;
  /** 命中返回插入下标；不在该列表范围内返回 null。 */
  hitTest: (clientX: number, clientY: number) => number | null;
};

const controllers: DropController[] = [];
const layers: DropLayer[] = [];

/** 注册一个文件队列；同 id 重复注册只更新，不打乱「默认 controller」的顺序。 */
export function registerDropController(controller: DropController): () => void {
  const index = controllers.findIndex((item) => item.id === controller.id);
  if (index >= 0) controllers[index] = { ...controllers[index], ...controller };
  else controllers.push(controller);
  return () => {
    const at = controllers.findIndex((item) => item.id === controller.id);
    if (at >= 0) controllers.splice(at, 1);
  };
}

/** 注册一个挂载中的列表视图；后注册的在上层，命中测试优先使用它。 */
export function registerDropLayer(layer: DropLayer): () => void {
  const index = layers.findIndex((item) => item.id === layer.id);
  if (index >= 0) layers[index] = layer;
  else layers.push(layer);
  return () => {
    const at = layers.findIndex((item) => item.id === layer.id);
    if (at >= 0) layers.splice(at, 1);
  };
}

/** 命中的 controller + 插入下标；下标为 null 表示追加到末尾。 */
export function resolveDropTarget(
  point: DropPoint | null,
): { controller: DropController; index: number | null } | null {
  if (!controllers.length) return null;
  if (point) {
    for (let i = layers.length - 1; i >= 0; i -= 1) {
      const layer = layers[i];
      const index = layer.hitTest(point.x, point.y);
      if (index == null) continue;
      const controller = controllers.find((item) => item.id === layer.controllerId);
      if (controller) return { controller, index };
    }
  }
  return { controller: controllers[0], index: null };
}

/** 把一次系统拖放投递给命中的队列；没有任何队列时返回 false。 */
export async function routeDrop(paths: string[], point: DropPoint | null): Promise<boolean> {
  const target = resolveDropTarget(point);
  if (!target) return false;
  await target.controller.addPaths(paths, target.index == null ? undefined : target.index);
  return true;
}

/** 仅供单测：清空注册表，避免用例之间互相污染。 */
export function resetDropRegistry(): void {
  controllers.length = 0;
  layers.length = 0;
}

import { formatBytes, formatDuration } from "@formatforge/design-tokens";
import type { Job } from "../lib/api";
import type { TaskQueueStats } from "@formatforge/task-queue";
import { Icon } from "./Icon";

const STATUS_LABEL: Record<Job["status"], string> = {
  queued: "排队中",
  running: "转换中",
  done: "已完成",
  failed: "失败",
  cancelled: "已取消",
  skipped: "已跳过",
};

export function TaskCenter({
  tasks,
  stats,
  concurrency,
  onConcurrency,
  paused,
  onPause,
  onCancel,
  onRetry,
  onOpenOutput,
  onOpenLog,
  onClearFinished,
  onClose,
}: {
  tasks: Job[];
  stats: TaskQueueStats;
  concurrency: number;
  onConcurrency: (value: number) => void;
  paused: boolean;
  onPause: (paused: boolean) => void;
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onOpenOutput: (id: string) => void;
  onOpenLog: (id: string) => void;
  onClearFinished: () => void;
  onClose: () => void;
}) {
  const ordered = tasks.slice().sort((a, b) => b.createdAt - a.createdAt);

  return (
    <aside className="taskcenter">
      <div className="tc-head">
        <Icon name="grid" size={15} />
        <b>任务中心</b>
        <span className="spacer" />
        <button
          className={`btn small ${paused ? "primary" : "ghost"}`}
          onClick={() => onPause(!paused)}
          title={paused ? "继续调度队列中的任务" : "暂停调度：不影响已在运行的任务"}
        >
          {paused ? "继续队列" : "暂停队列"}
        </button>
        <span className="muted" style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
          并发
        </span>
        <select
          className="select"
          style={{ width: 64, height: 26 }}
          value={concurrency}
          onChange={(event) => onConcurrency(Number(event.target.value))}
          title="同时运行的任务数"
        >
          {[1, 2, 3, 4, 5, 6, 7, 8].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
        <button className="iconbtn" onClick={onClose} title="收起任务中心">
          <Icon name="close" size={13} />
        </button>
      </div>

      <div className="tc-stats">
        <div className="stat">
          <b>{stats.total}</b>
          <span>全部</span>
        </div>
        <div className="stat run">
          <b>{stats.running}</b>
          <span>进行中</span>
        </div>
        <div className="stat">
          <b>{stats.done}</b>
          <span>已完成</span>
        </div>
        <div className="stat fail">
          <b>{stats.failed}</b>
          <span>失败</span>
        </div>
        <div className="stat">
          <b>{stats.skipped}</b>
          <span>跳过</span>
        </div>
      </div>

      {paused ? (
        <div className="note" style={{ margin: "0 12px 8px" }}>
          队列已暂停：不会启动新任务，正在运行的任务会继续跑完。点「继续队列」恢复调度。
        </div>
      ) : null}

      <div className="tc-body">
        {ordered.length ? (
          ordered.map((task) => {
            const percent = Math.round(Math.max(0, Math.min(1, task.progress)) * 100);
            const active = task.status === "running" || task.status === "queued";
            return (
              <div key={task.id} className={`task ${task.status}`}>
                <div className="task-top">
                  <span className="nm" title={`${task.input}\n→ ${task.output}`}>
                    {task.inputName}
                  </span>
                  <span className="st">{STATUS_LABEL[task.status]}</span>
                </div>
                <div className="progress">
                  <i style={{ width: `${task.status === "done" ? 100 : percent}%` }} />
                </div>
                <div className="task-meta">
                  <span>
                    {task.status === "running" ? `${percent}%` : task.status === "done" ? "完成" : STATUS_LABEL[task.status]}
                  </span>
                  {task.speed ? <span>{task.speed}</span> : null}
                  {task.status === "running" && task.etaSec ? <span>剩余 {formatDuration(task.etaSec)}</span> : null}
                  {task.attempts > 1 ? <span>第 {task.attempts + 1} 次</span> : null}
                  <span className="spacer" />
                  {task.outputSize ? <span>{formatBytes(task.outputSize)}</span> : null}
                </div>
                {task.encoderUsed ? (
                  <div className="task-meta" style={{ marginTop: 4 }}>
                    <span>编码器 {task.encoderUsed}</span>
                  </div>
                ) : null}
                {task.fallbackNote ? <div className="task-msg fallback">回退：{task.fallbackNote}</div> : null}
                {task.message ? (
                  <div className={`task-msg ${task.status === "failed" ? "error" : ""}`}>{task.message}</div>
                ) : null}
                {task.errorKind ? (
                  <div className="task-meta" style={{ marginTop: 4 }}>
                    <span>错误类型：{task.errorKind}</span>
                  </div>
                ) : null}
                <div className="task-actions">
                  {active ? (
                    <button className="btn small danger" onClick={() => onCancel(task.id)}>
                      取消
                    </button>
                  ) : null}
                  {task.status === "failed" || task.status === "cancelled" ? (
                    <button className="btn small" onClick={() => onRetry(task.id)}>
                      <Icon name="retry" size={12} />
                      重试
                    </button>
                  ) : null}
                  {task.status === "done" || task.status === "failed" ? (
                    <button className="btn small ghost" onClick={() => onOpenOutput(task.id)}>
                      <Icon name="folder" size={12} />
                      打开目录
                    </button>
                  ) : null}
                  {task.logPath ? (
                    <button className="btn small ghost" onClick={() => onOpenLog(task.id)} title={task.logPath}>
                      <Icon name="document" size={12} />
                      日志
                    </button>
                  ) : null}
                </div>
              </div>
            );
          })
        ) : (
          <div className="empty">
            <div className="big">
              <Icon name="grid" size={22} />
            </div>
            <div>还没有任务</div>
            <div style={{ fontSize: 10.5, opacity: 0.8 }}>选择一条转换方法并添加文件即可开始</div>
          </div>
        )}
      </div>

      <div className="inspector-foot" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <span className="muted" style={{ fontSize: 10.5, color: "var(--ff-fg3)" }}>
          排队 {stats.queued} · 取消 {stats.cancelled}
        </span>
        <button className="btn small ghost" style={{ marginLeft: "auto" }} onClick={onClearFinished}>
          清空已完成
        </button>
      </div>
    </aside>
  );
}

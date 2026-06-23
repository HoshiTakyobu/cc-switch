import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { providersApi, type AppId } from "@/lib/api";
import type { Provider } from "@/types";
import { toast } from "sonner";

interface RunningTerminal {
  pid: number;
  name: string;
  commandLine: string;
  startedAt: number | null;
  boundProviderId: string | null;
  strict: boolean;
}

/**
 * 终端绑定面板（2B）：列出当前渠道正在运行、经 cc-switch 代理(15721)的 CLI 终端，
 * 每个可指派一个源；代理按连接 PID 路由到绑定源，从而多终端分摊各上游 RPM。
 */
export function TerminalsPanel({ appId }: { appId: AppId }) {
  const [rows, setRows] = useState<RunningTerminal[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<RunningTerminal[]>("list_running_terminals", {
        app: appId,
      });
      setRows(list);
    } catch {
      // 枚举失败静默处理，不打扰
    }
  }, [appId]);

  useEffect(() => {
    providersApi
      .getAll(appId)
      .then((map) => setProviders(Object.values(map)))
      .catch(() => {});
  }, [appId]);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [refresh]);

  const assign = async (
    row: RunningTerminal,
    providerId: string,
    strict: boolean,
  ) => {
    try {
      if (!providerId) {
        await invoke("clear_terminal_binding", { pid: row.pid, app: appId });
        toast.success("已解绑");
      } else {
        await invoke("set_terminal_binding", {
          pid: row.pid,
          app: appId,
          providerId,
          strict,
        });
        toast.success("已绑定");
      }
      refresh();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="px-6 flex flex-col flex-1 min-h-0 overflow-hidden">
      <p className="text-sm text-muted-foreground mb-3 leading-relaxed">
        把正在运行的 {appId} 终端（经 cc-switch 代理 15721）各自绑定到一个源，分摊各上游
        RPM。严格模式下，绑定源不可用时该终端直接报错、不串到别的源；非严格则回退全局故障队列。
        未绑定的终端按全局逻辑（当前源 + 故障队列）走。
      </p>
      <div className="flex-1 overflow-y-auto overflow-x-hidden pb-12">
        {rows.length === 0 ? (
          <div className="text-sm text-muted-foreground py-10 text-center">
            没有检测到正在运行的 {appId} 终端。
            <br />
            打开一个走 cc-switch（15721）的 {appId} 终端后会自动出现在这里。
          </div>
        ) : (
          <table className="w-full text-sm border-collapse">
            <thead>
              <tr className="text-left text-muted-foreground border-b">
                <th className="py-2 pr-3 font-medium">PID</th>
                <th className="py-2 pr-3 font-medium">进程</th>
                <th className="py-2 pr-3 font-medium">绑定源</th>
                <th className="py-2 pr-3 font-medium">严格</th>
                <th className="py-2 pr-3 font-medium">操作</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.pid}
                  className="border-b hover:bg-black/5 dark:hover:bg-white/5"
                >
                  <td className="py-2 pr-3 font-mono">{row.pid}</td>
                  <td
                    className="py-2 pr-3 max-w-[280px] truncate"
                    title={row.commandLine}
                  >
                    {row.name}
                  </td>
                  <td className="py-2 pr-3">
                    <select
                      className="border rounded px-2 py-1 bg-background min-w-[180px]"
                      value={row.boundProviderId ?? ""}
                      onChange={(e) => assign(row, e.target.value, row.strict)}
                    >
                      <option value="">（未绑定 / 走全局）</option>
                      {providers.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td className="py-2 pr-3">
                    <input
                      type="checkbox"
                      checked={row.strict}
                      disabled={!row.boundProviderId}
                      onChange={(e) =>
                        row.boundProviderId &&
                        assign(row, row.boundProviderId, e.target.checked)
                      }
                    />
                  </td>
                  <td className="py-2 pr-3">
                    {row.boundProviderId && (
                      <button
                        type="button"
                        className="text-red-600 hover:underline"
                        onClick={() => assign(row, "", false)}
                      >
                        解绑
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

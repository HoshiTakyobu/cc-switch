import { invoke } from "@tauri-apps/api/core";
import type { AppId } from "./types";

// ===== 连通性检查类型 =====
// 注意：本检查只探测 base_url 是否可达，不发真实大模型请求，也不触碰故障转移熔断器。

export type HealthStatus = "operational" | "degraded" | "failed";

export interface StreamCheckConfig {
  /** 单次探测超时（秒） */
  timeoutSecs: number;
  /** 超时类失败的最大重试次数 */
  maxRetries: number;
  /** 降级阈值（毫秒）：可达但 TTFB 超过该值判定为"较慢" */
  degradedThresholdMs: number;
}

export interface StreamCheckResult {
  status: HealthStatus;
  success: boolean;
  message: string;
  responseTimeMs?: number;
  httpStatus?: number;
  testedAt: number;
  retryCount: number;
}

// ===== 连通性检查 API =====

/**
 * 连通性检查（单个供应商）
 */
export async function streamCheckProvider(
  appType: AppId,
  providerId: string,
): Promise<StreamCheckResult> {
  return invoke("stream_check_provider", { appType, providerId });
}

/**
 * 批量流式健康检查
 */
export async function streamCheckAllProviders(
  appType: AppId,
  proxyTargetsOnly: boolean = false,
): Promise<Array<[string, StreamCheckResult]>> {
  return invoke("stream_check_all_providers", { appType, proxyTargetsOnly });
}

/**
 * 获取流式检查配置
 */
export async function getStreamCheckConfig(): Promise<StreamCheckConfig> {
  return invoke("get_stream_check_config");
}

/**
 * 保存流式检查配置
 */
export async function saveStreamCheckConfig(
  config: StreamCheckConfig,
): Promise<void> {
  return invoke("save_stream_check_config", { config });
}

// ===== 真实模型测试 =====
// 与可达性检查不同：发送一次真实最小模型请求，返回真实 HTTP 状态码与错误分类。
// 显式用户操作，可能产生极小额计费（max_tokens=1）。不触碰故障转移熔断器。

export type RealCheckErrorCategory =
  | "auth"
  | "forbidden"
  | "not_found"
  | "rate_limit"
  | "server"
  | "network"
  | "unknown";

export interface RealCheckResult {
  success: boolean;
  message: string;
  httpStatus?: number;
  modelUsed: string;
  responseTimeMs?: number;
  errorCategory?: RealCheckErrorCategory;
  testedAt: number;
}

/**
 * 真实模型测试（单个供应商）。
 * @param model 可选；不传则后端从供应商配置推断测试模型。
 */
export async function realCheckProvider(
  appType: AppId,
  providerId: string,
  model?: string,
  timeoutSecs?: number,
): Promise<RealCheckResult> {
  return invoke("real_check_provider", {
    appType,
    providerId,
    model,
    timeoutSecs,
  });
}

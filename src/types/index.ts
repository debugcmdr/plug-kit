export interface PluginInfo {
  id: string
  name: string
  version: string
  description: string
  is_installed: boolean
  installed_version: string | null
}

export interface InstallResult {
  success: boolean
  plugin_id: string
  message: string
  /** 结构化错误码:ALREADY_LATEST / BRIDGE_INCOMPATIBLE / MIN_APP_VERSION / UNSIGNED_PLUGIN 等 */
  code?: string
}

export interface Task {
  task_id: string
  plugin_id: string
  type: string
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled' | 'interrupted'
  progress: {
    percent: number
    speed?: string
    eta?: string
    message?: string
  }
  /** 失败原因(失败任务展示) */
  error?: string
  /** 转换文件名(插件 progress 上报,任务中心展示) */
  file_name?: string
  /** 输出文件/目录路径(任务中心"打开输出文件夹"按钮使用) */
  output_path?: string
  /** 任务来源链接(提取类任务记录,用于去重/排查) */
  url?: string
  created_at: string
  started_at?: string
  completed_at?: string
}

export interface CacheStats {
  total_size_bytes: number
  total_size_mb: number
  entry_count: number
}

export interface BridgeConfig {
  version: string
}

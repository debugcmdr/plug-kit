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
  }
  created_at: string
  started_at?: string
  completed_at?: string
}

export interface Settings {
  download_mirrors: MirrorConfig[]
  system_proxy?: ProxyConfig
  disk_quota_mb: number
}

export interface MirrorConfig {
  name: string
  prefix: string
  enabled: boolean
}

export interface ProxyConfig {
  enabled: boolean
  host: string
  port: number
}

export interface CacheStats {
  total_size_bytes: number
  total_size_mb: number
  entry_count: number
  quota_mb: number
}

export interface BridgeConfig {
  version: string
}

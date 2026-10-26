import { useState, useEffect } from 'react'
import { getAuthHeaders } from '../lib/auth'

/**
 * 系统状态组件
 */
export function SystemStatus({ onError }) {
  const [systemStatus, setSystemStatus] = useState(null)
  const [systemMetrics, setSystemMetrics] = useState(null)
  const [isLoading, setIsLoading] = useState(false)

  const fetchData = async () => {
    setIsLoading(true)
    try {
      // 获取推理引擎状态
      const inferenceResponse = await fetch('/api/inference/status', {
        headers: getAuthHeaders(),
      })
      const inference = inferenceResponse.ok ? await inferenceResponse.json() : null
      
      // 获取系统指标
      const metricsResponse = await fetch('/api/system/metrics', {
        headers: getAuthHeaders(),
      })
      const metrics = metricsResponse.ok ? await metricsResponse.json() : null
      
      setSystemStatus({
        health: inference ? 'ok' : 'error',
        inference: inference,
      })
      
      if (metrics) {
        setSystemMetrics(metrics)
      }
    } catch (e) {
      onError('获取系统状态失败')
    } finally {
      setIsLoading(false)
    }
  }

  useEffect(() => {
    fetchData()
    
    // 每30秒刷新一次系统指标
    const interval = setInterval(fetchData, 30000)
    return () => clearInterval(interval)
  }, [])

  const formatBytes = (bytes) => {
    if (!bytes) return '-'
    const gb = bytes / (1024 * 1024 * 1024)
    return `${gb.toFixed(2)} GB`
  }

  // 根据使用率返回颜色
  const getUsageColor = (percent) => {
    if (percent < 50) return 'bg-green-500'
    if (percent < 80) return 'bg-orange-500'
    return 'bg-red-500'
  }

  return (
    <div className="space-y-4">
      {/* 软件信息 */}
      <div className="p-4 rounded-lg border border-border">
        <h3 className="font-medium text-foreground mb-3">软件信息</h3>
        {isLoading && !systemMetrics ? (
          <p className="text-sm text-muted-foreground">加载中...</p>
        ) : systemMetrics ? (
          <div className="grid grid-cols-3 gap-2 text-sm">
            <p><span className="text-foreground/70">软件名称:</span> <span className="text-foreground font-medium">{systemMetrics.app_name}</span></p>
            <p><span className="text-foreground/70">版本:</span> <span className="text-foreground font-medium">v{systemMetrics.app_version}</span></p>
            <p><span className="text-foreground/70">发布日期:</span> <span className="text-foreground font-medium">{systemMetrics.app_release_date}</span></p>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">无法获取软件信息</p>
        )}
      </div>

      {/* 系统资源 */}
      <div className="p-4 rounded-lg border border-border">
        <h3 className="font-medium text-foreground mb-3">系统资源</h3>
        {isLoading && !systemMetrics ? (
          <p className="text-sm text-muted-foreground">加载中...</p>
        ) : systemMetrics ? (
          <div className="space-y-3">
            {/* CPU 和 内存 - 一排显示 */}
            <div className="grid grid-cols-2 gap-4">
              {/* CPU */}
              <div>
                <div className="flex justify-between items-center mb-1">
                  <span className="text-sm text-foreground">CPU 使用率</span>
                  <span className="text-sm font-medium text-foreground">{systemMetrics.cpu_usage_percent.toFixed(1)}%</span>
                </div>
                <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                  <div 
                    className={`${getUsageColor(systemMetrics.cpu_usage_percent)} h-2 rounded-full transition-all duration-500`}
                    style={{ width: `${systemMetrics.cpu_usage_percent}%` }}
                  />
                </div>
                {systemMetrics.cpu_temp_c && (
                  <p className="text-xs text-muted-foreground mt-1">温度: {systemMetrics.cpu_temp_c.toFixed(1)}°C</p>
                )}
              </div>

              {/* 内存 */}
              <div>
                <div className="flex justify-between items-center mb-1">
                  <span className="text-sm text-foreground">内存使用率</span>
                  <span className="text-sm font-medium text-foreground">{systemMetrics.mem_used_percent.toFixed(1)}%</span>
                </div>
                <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                  <div 
                    className={`${getUsageColor(systemMetrics.mem_used_percent)} h-2 rounded-full transition-all duration-500`}
                    style={{ width: `${systemMetrics.mem_used_percent}%` }}
                  />
                </div>
                <p className="text-xs text-muted-foreground mt-1">
                  {formatBytes(systemMetrics.mem_total_bytes - systemMetrics.mem_available_bytes)} / {formatBytes(systemMetrics.mem_total_bytes)}
                </p>
              </div>
            </div>

            {/* GPU */}
            {systemMetrics.gpu_info && (
              <div className="pt-2 border-t border-border">
                <div className="flex items-center gap-2 mb-3">
                  <span className="text-sm font-medium text-foreground">GPU:</span>
                  <span className="text-sm text-foreground">{systemMetrics.gpu_info.name}</span>
                </div>
                
                {/* GPU 利用率和显存 - 一排显示 */}
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <div className="flex justify-between items-center mb-1">
                      <span className="text-sm text-foreground/70">GPU 利用率</span>
                      <span className="text-sm font-medium text-foreground">{systemMetrics.gpu_info.utilization_percent.toFixed(1)}%</span>
                    </div>
                    <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                      <div 
                        className={`${getUsageColor(systemMetrics.gpu_info.utilization_percent)} h-2 rounded-full transition-all duration-500`}
                        style={{ width: `${systemMetrics.gpu_info.utilization_percent}%` }}
                      />
                    </div>
                  </div>

                  <div>
                    <div className="flex justify-between items-center mb-1">
                      <span className="text-sm text-foreground/70">显存使用率</span>
                      <span className="text-sm font-medium text-foreground">{systemMetrics.gpu_info.memory_used_percent.toFixed(1)}%</span>
                    </div>
                    <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                      <div 
                        className={`${getUsageColor(systemMetrics.gpu_info.memory_used_percent)} h-2 rounded-full transition-all duration-500`}
                        style={{ width: `${systemMetrics.gpu_info.memory_used_percent}%` }}
                      />
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      {formatBytes(systemMetrics.gpu_info.memory_used_bytes)} / {formatBytes(systemMetrics.gpu_info.memory_total_bytes)}
                    </p>
                  </div>
                </div>

                {systemMetrics.gpu_info.temperature_c && (
                  <p className="text-xs text-muted-foreground mt-2">温度: {systemMetrics.gpu_info.temperature_c.toFixed(1)}°C</p>
                )}
              </div>
            )}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">无法获取系统资源信息</p>
        )}
      </div>

      {/* 推理引擎 */}
      <div className="p-4 rounded-lg border border-border">
        <h3 className="font-medium text-foreground mb-3">推理引擎</h3>
        {isLoading && !systemStatus?.inference ? (
          <p className="text-sm text-muted-foreground">加载中...</p>
        ) : systemStatus?.inference ? (
          <div className="space-y-2 text-sm">
            <div className="flex items-center gap-2">
              <span className={`h-2 w-2 rounded-full ${systemStatus.inference.state === 'running' ? 'bg-green-500' : 'bg-red-500'}`} />
              <span className="text-foreground font-medium">
                {systemStatus.inference.state === 'running' ? '运行中' : systemStatus.inference.state || '未知'}
              </span>
            </div>
            <div className="grid grid-cols-2 gap-2 text-muted-foreground mt-3">
              <p><span className="text-foreground/70">引擎类型:</span> {systemStatus.inference.engine_type || '-'}</p>
              <p><span className="text-foreground/70">监听地址:</span> {systemStatus.inference.host}:{systemStatus.inference.port}</p>
              <p className="col-span-2"><span className="text-foreground/70">模型:</span> {systemStatus.inference.model_name || '-'}</p>
              {systemStatus.inference.mmproj_path && (
                <p className="col-span-2"><span className="text-foreground/70">视觉模型:</span> {systemStatus.inference.mmproj_path}</p>
              )}
              <p><span className="text-foreground/70">进程 PID:</span> {systemStatus.inference.pid || '-'}</p>
              <p><span className="text-foreground/70">重启次数:</span> {systemStatus.inference.restart_count ?? '-'}</p>
              {systemStatus.inference.start_time && (
                <p className="col-span-2"><span className="text-foreground/70">启动时间:</span> {new Date(systemStatus.inference.start_time).toLocaleString()}</p>
              )}
              {systemStatus.inference.last_error && (
                <p className="col-span-2 text-destructive"><span className="text-foreground/70">最后错误:</span> {systemStatus.inference.last_error}</p>
              )}
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">未运行</p>
        )}
      </div>
    </div>
  )
}

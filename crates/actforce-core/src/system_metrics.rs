use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use sysinfo::{Components, MemoryRefreshKind, RefreshKind, System};

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct SystemMetricsResp {
    pub ok: bool,
    pub app_name: String,
    pub app_version: String,
    pub app_release_date: String,
    pub cpu_usage_percent: f64,
    pub mem_total_bytes: u64,
    pub mem_available_bytes: u64,
    pub mem_used_percent: f64,
    pub cpu_temp_c: Option<f64>,
    pub gpu_info: Option<GpuInfo>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub utilization_percent: f64,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub memory_used_percent: f64,
    pub temperature_c: Option<f64>,
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 获取 CPU 使用率
/// 使用 sysinfo 库，更准确且跨平台
async fn cpu_usage_percent() -> anyhow::Result<f64> {
    let mut sys = System::new();
    
    // sysinfo 需要至少调用一次 refresh_cpu 才能获取到使用率
    // 第一次初始化
    sys.refresh_cpu();
    
    // 等待一小段时间让系统更新计数器
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    
    // 第二次刷新，获取实际使用率
    sys.refresh_cpu();
    
    // 计算全局 CPU 使用率
    let cpu = sys.global_cpu_info();
    let usage = cpu.cpu_usage() as f64;
    Ok(usage.clamp(0.0, 100.0))
}

/// 获取内存信息
/// 使用 sysinfo 库，更准确且跨平台
async fn mem_metrics() -> anyhow::Result<(u64, u64, f64)> {
    let sys = System::new_with_specifics(
        RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram())
    );
    
    let total = sys.total_memory();
    let available = sys.available_memory();
    let used = total.saturating_sub(available);
    
    let used_percent = if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    };
    
    Ok((total, available, used_percent))
}

/// 获取 CPU 温度
/// 使用 sysinfo 的 Components，自动处理各种传感器
/// 优先读取 k10temp (AMD) 或 coretemp (Intel)
async fn cpu_temp_c() -> Option<f64> {
    let components = Components::new_with_refreshed_list();
    
    if components.is_empty() {
        return None;
    }
    
    let mut max_temp: Option<f64> = None;
    
    // 优先查找 CPU 温度传感器
    let cpu_sensor_labels = ["k10temp", "coretemp", "k8temp", "Tctl", "Package id"];
    
    for component in &components {
        let label = component.label().to_lowercase();
        
        // 检查是否是 CPU 传感器
        let is_cpu_sensor = cpu_sensor_labels.iter().any(|&cpu_label| {
            label.contains(&cpu_label.to_lowercase())
        });
        
        if is_cpu_sensor {
            let temp = component.temperature() as f64;
            // 合理的 CPU 温度范围：0-120°C
            if (0.0..120.0).contains(&temp) {
                max_temp = Some(max_temp.map(|m| m.max(temp)).unwrap_or(temp));
            }
        }
    }
    
    // 如果找到 CPU 温度，直接返回
    if max_temp.is_some() {
        return max_temp;
    }
    
    // 如果没有找到CPU专用传感器，尝试读取其他合理的温度
    for component in &components {
        let label = component.label().to_lowercase();
        
        // 跳过明显不是CPU的传感器
        if label.contains("wifi") || label.contains("battery") || 
           label.contains("acpitz") || label.contains("pkg") {
            continue;
        }
        
        let temp = component.temperature() as f64;
        // 合理的温度范围：20-120°C
        if (20.0..120.0).contains(&temp) {
            max_temp = Some(max_temp.map(|m| m.max(temp)).unwrap_or(temp));
        }
    }
    
    max_temp
}

/// 获取 GPU 信息
/// 通过 nvidia-smi 命令获取（sysinfo 不直接支持 GPU）
async fn get_gpu_info() -> Option<GpuInfo> {
    let output = tokio::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,memory.total,memory.used,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    
    // Parse CSV: name, util_gpu, mem_total, mem_used, temp_gpu
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 5 {
        return None;
    }

    let name = parts[0].to_string();
    let utilization: f64 = parts[1].parse().ok()?;
    let mem_total: f64 = parts[2].parse().ok()?;
    let mem_used: f64 = parts[3].parse().ok()?;
    let temp: f64 = parts[4].parse().ok()?;

    let mem_total_bytes = (mem_total * 1024.0 * 1024.0) as u64;
    let mem_used_bytes = (mem_used * 1024.0 * 1024.0) as u64;
    let mem_used_percent = if mem_total > 0.0 {
        (mem_used / mem_total * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    Some(GpuInfo {
        name,
        utilization_percent: utilization.clamp(0.0, 100.0),
        memory_total_bytes: mem_total_bytes,
        memory_used_bytes: mem_used_bytes,
        memory_used_percent: mem_used_percent,
        temperature_c: if temp > 0.0 { Some(temp) } else { None },
    })
}

pub async fn get_system_metrics(
    State(state): State<AppState>,
) -> Result<Json<SystemMetricsResp>, (StatusCode, String)> {
    let _ = state; // State is available for future use

    let cpu = cpu_usage_percent()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (mem_total, mem_avail, mem_used_percent) = mem_metrics()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let temp = cpu_temp_c().await;
    let mut gpu_info = get_gpu_info().await;

    // 默认隐藏 GPU 型号和显存大小，仅当 ACTFORCE_SHOW_SENSITIVE_INFO=true 时显示
    let show_sensitive = std::env::var("ACTFORCE_SHOW_SENSITIVE_INFO")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);
    if !show_sensitive {
        if let Some(ref mut gpu) = gpu_info {
            gpu.name.clear();
            gpu.memory_total_bytes = 0;
            gpu.memory_used_bytes = 0;
        }
    }

    Ok(Json(SystemMetricsResp {
        ok: true,
        app_name: "ActForce".to_string(),
        app_version: crate::config::VERSION.to_string(),
        app_release_date: crate::config::RELEASE_DATE.to_string(),
        cpu_usage_percent: cpu,
        mem_total_bytes: mem_total,
        mem_available_bytes: mem_avail,
        mem_used_percent,
        cpu_temp_c: temp,
        gpu_info,
        created_at: now_epoch(),
    }))
}

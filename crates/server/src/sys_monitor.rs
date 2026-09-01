use std::sync::Arc;
use tokio::sync::Mutex;
use sysinfo::{System, Cpu, Disk, NetworkData, Networks, Disks};
use shared::{SystemHealth, ContainerMetric};
use bollard::Docker;
use bollard::container::ListContainersOptions;
use futures_util::StreamExt;

pub struct SysMonitor {
    sys: Arc<Mutex<System>>,
    docker: Option<Docker>,
}

impl SysMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        
        // Attempt to connect to Docker
        let docker = Docker::connect_with_local_defaults().ok();

        Self {
            sys: Arc::new(Mutex::new(sys)),
            docker,
        }
    }

    pub async fn get_system_health(&self) -> SystemHealth {
        let mut sys_guard = self.sys.lock().await;
        sys_guard.refresh_all();
        let sys = &*sys_guard;

        let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());
        let cpu_usage_total = sys.global_cpu_info().cpu_usage();
        let cpu_cores = sys.cpus().iter().map(|cpu| cpu.cpu_usage()).collect();
        
        let memory_total_kb = sys.total_memory();
        let memory_used_kb = sys.used_memory();
        let memory_free_kb = sys.free_memory();
        
        let swap_total_kb = sys.total_swap();
        let swap_used_kb = sys.used_swap();

        // Disks
        let mut disk_total_bytes = 0;
        let mut disk_free_bytes = 0;
        let disks = Disks::new_with_refreshed_list();
        for disk in disks.list() {
            disk_total_bytes += disk.total_space();
            disk_free_bytes += disk.available_space();
        }

        // Uptime
        let uptime_seconds = System::uptime();

        // Load avg
        let load = System::load_average();
        let load_average = (load.one, load.five, load.fifteen);

        // Network
        let mut network_rx_bytes_sec = 0;
        let mut network_tx_bytes_sec = 0;
        let networks = Networks::new_with_refreshed_list();
        for (_interface, data) in &networks {
            network_rx_bytes_sec += data.received();
            network_tx_bytes_sec += data.transmitted();
        }

        SystemHealth {
            hostname,
            cpu_usage_total,
            cpu_cores,
            memory_total_kb,
            memory_used_kb,
            memory_free_kb,
            swap_total_kb,
            swap_used_kb,
            disk_total_bytes,
            disk_free_bytes,
            disk_read_bytes_sec: 0,
            disk_write_bytes_sec: 0,
            uptime_seconds,
            load_average,
            network_rx_bytes_sec,
            network_tx_bytes_sec,
        }
    }

    pub async fn get_container_metrics(&self) -> Vec<ContainerMetric> {
        let docker = match &self.docker {
            Some(d) => d,
            None => return Vec::new(),
        };

        let mut metrics = Vec::new();
        
        let options = Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        });

        if let Ok(containers) = docker.list_containers(options).await {
            for container in containers {
                let id = container.id.unwrap_or_default();
                let name = container.names.unwrap_or_default().first().cloned().unwrap_or_default();
                let status = container.state.unwrap_or_else(|| "unknown".to_string());
                
                let mut cpu_usage_percent = 0.0;
                let mut memory_usage_bytes = 0;
                let mut memory_limit_bytes = 0;

                if status == "running" {
                    let mut stats_stream = docker.stats(&id, Some(bollard::container::StatsOptions {
                        stream: false,
                        one_shot: true,
                    }));

                    if let Some(Ok(stats)) = stats_stream.next().await {
                        // Calculate memory
                        let mem_stats = stats.memory_stats;
                        memory_usage_bytes = mem_stats.usage.unwrap_or(0);
                        memory_limit_bytes = mem_stats.limit.unwrap_or(0);

                        // Calculate CPU usage percent
                        let cpu_stats = stats.cpu_stats;
                        let precpu_stats = stats.precpu_stats;
                        let cpu_delta = cpu_stats.cpu_usage.total_usage as f64 - precpu_stats.cpu_usage.total_usage as f64;
                        let system_delta = (cpu_stats.system_cpu_usage.unwrap_or(0) - precpu_stats.system_cpu_usage.unwrap_or(0)) as f64;
                        let num_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;

                        if system_delta > 0.0 && cpu_delta > 0.0 {
                            cpu_usage_percent = (cpu_delta / system_delta) * num_cpus * 100.0;
                        }
                    }
                }

                metrics.push(ContainerMetric {
                    id,
                    name,
                    cpu_usage_percent,
                    memory_usage_bytes,
                    memory_limit_bytes,
                    status,
                });
            }
        }

        metrics
    }
}

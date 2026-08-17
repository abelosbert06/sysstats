#[cfg(not(target_arch = "wasm32"))]
pub mod desktop {
    use rusqlite::{params, Connection, Result};
    use shared_types::SystemMetricPayload;
    use std::path::PathBuf;

    fn get_config_dir() -> Option<std::path::PathBuf> {
        let mut path = dirs::data_dir()?;
        path.push("sys_stats");
        std::fs::create_dir_all(&path).ok()?;
        Some(path)
    }

    pub fn save_device_token(token: &str) -> Result<(), String> {
        let mut path = get_config_dir().ok_or("Could not find data directory")?;
        path.push("device_token");
        std::fs::write(&path, token).map_err(|e| e.to_string())
    }

    pub fn load_device_token() -> Option<String> {
        let mut path = get_config_dir()?;
        path.push("device_token");
        std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn delete_device_token() -> Result<(), String> {
        let mut path = get_config_dir().ok_or("Could not find data directory")?;
        path.push("device_token");
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }

    fn get_db_path() -> Result<PathBuf, String> {
        let mut path = dirs::data_dir().ok_or_else(|| "Could not find data directory".to_string())?;
        path.push("sys_stats");
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        path.push("metrics.db");
        Ok(path)
    }

    pub fn get_connection() -> Result<Connection, String> {
        let db_path = get_db_path()?;
        Connection::open(db_path).map_err(|e| e.to_string())
    }

    pub fn init_db() -> Result<(), String> {
        let conn = get_connection()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS local_metrics_buffer (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                timestamp_sec INTEGER NOT NULL,
                cpu_usage_pct REAL NOT NULL,
                memory_used_mb INTEGER NOT NULL,
                memory_total_mb INTEGER NOT NULL,
                network_rx_bytes_sec INTEGER NOT NULL,
                network_tx_bytes_sec INTEGER NOT NULL,
                cpu_temperature_c REAL NOT NULL,
                disk_usage_pct REAL NOT NULL,
                disk_read_bytes_sec INTEGER NOT NULL DEFAULT 0,
                disk_written_bytes_sec INTEGER NOT NULL DEFAULT 0,
                uptime_sec INTEGER NOT NULL,
                running_processes INTEGER NOT NULL,
                top_processes TEXT NOT NULL DEFAULT '[]'
            )",
            [],
        )
        .map_err(|e| e.to_string())?;
        
        // Ensure the column exists for existing databases (schema evolution)
        let _ = conn.execute("ALTER TABLE local_metrics_buffer ADD COLUMN top_processes TEXT NOT NULL DEFAULT '[]'", []);
        let _ = conn.execute("ALTER TABLE local_metrics_buffer ADD COLUMN disk_read_bytes_sec INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE local_metrics_buffer ADD COLUMN disk_written_bytes_sec INTEGER NOT NULL DEFAULT 0", []);
        Ok(())
    }

    pub fn insert_metric(payload: &SystemMetricPayload) -> Result<(), String> {
        let conn = get_connection()?;
        let top_processes_json = serde_json::to_string(&payload.processes).map_err(|e| e.to_string())?;
        
        conn.execute(
            "INSERT INTO local_metrics_buffer (
                device_id, timestamp_sec, cpu_usage_pct, memory_used_mb, memory_total_mb,
                network_rx_bytes_sec, network_tx_bytes_sec, cpu_temperature_c, disk_usage_pct,
                disk_read_bytes_sec, disk_written_bytes_sec, uptime_sec, running_processes, top_processes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                payload.device_id,
                payload.timestamp_sec as i64,
                payload.cpu_usage_pct,
                payload.memory_used_mb as i64,
                payload.memory_total_mb as i64,
                payload.network_rx_bytes_sec as i64,
                payload.network_tx_bytes_sec as i64,
                payload.cpu_temperature_c,
                payload.disk_usage_pct,
                payload.disk_read_bytes_sec as i64,
                payload.disk_written_bytes_sec as i64,
                payload.uptime_sec as i64,
                payload.running_processes as i64,
                top_processes_json,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_un_synced_metrics(limit: usize) -> Result<Vec<(i64, SystemMetricPayload)>, String> {
        let conn = get_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, device_id, timestamp_sec, cpu_usage_pct, memory_used_mb, memory_total_mb,
                        network_rx_bytes_sec, network_tx_bytes_sec, cpu_temperature_c, disk_usage_pct,
                        disk_read_bytes_sec, disk_written_bytes_sec, uptime_sec, running_processes, top_processes
                 FROM local_metrics_buffer
                 ORDER BY id ASC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let id: i64 = row.get(0)?;
                let timestamp_sec: i64 = row.get(2)?;
                let memory_used_mb: i64 = row.get(4)?;
                let memory_total_mb: i64 = row.get(5)?;
                let network_rx_bytes_sec: i64 = row.get(6)?;
                let network_tx_bytes_sec: i64 = row.get(7)?;
                let disk_read_bytes_sec: i64 = row.get(10)?;
                let disk_written_bytes_sec: i64 = row.get(11)?;
                let uptime_sec: i64 = row.get(12)?;
                let running_processes: i64 = row.get(13)?;
                let top_processes_str: String = row.get(14)?;
                let top_processes = serde_json::from_str(&top_processes_str).unwrap_or_default();

                Ok((
                    id,
                    SystemMetricPayload {
                        device_id: row.get(1)?,
                        timestamp_sec: timestamp_sec as u64,
                        cpu_usage_pct: row.get(3)?,
                        memory_used_mb: memory_used_mb as u64,
                        memory_total_mb: memory_total_mb as u64,
                        network_rx_bytes_sec: network_rx_bytes_sec as u64,
                        network_tx_bytes_sec: network_tx_bytes_sec as u64,
                        cpu_temperature_c: row.get(8)?,
                        disk_usage_pct: row.get(9)?,
                        disk_read_bytes_sec: disk_read_bytes_sec as u64,
                        disk_written_bytes_sec: disk_written_bytes_sec as u64,
                        uptime_sec: uptime_sec as u64,
                        running_processes: running_processes as u32,
                        processes: top_processes,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for row in rows {
            if let Ok(item) = row {
                results.push(item);
            }
        }
        Ok(results)
    }

    pub fn delete_synced_metrics(ids: &[i64]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let conn = get_connection()?;
        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let query = format!(
            "DELETE FROM local_metrics_buffer WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        stmt.execute(params.as_slice()).map_err(|e| e.to_string())?;
        Ok(())
    }
}

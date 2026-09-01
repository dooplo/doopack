use bollard::Docker;
use bollard::container::{CreateContainerOptions, StartContainerOptions, Config, RemoveContainerOptions};
use bollard::image::BuildImageOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::fs;
use futures_util::stream::StreamExt;

#[derive(Clone)]
pub struct DockerManager {
    pub docker: Docker,
}

pub fn extract_source(source_type: &str, source_code: &str, dest_dir: &std::path::Path) -> Result<(), String> {
    if source_type == "github" {
        let status = Command::new("git")
            .args(&["clone", source_code, "."])
            .current_dir(dest_dir)
            .status()
            .map_err(|e| format!("Failed to run git clone: {}", e))?;
        if !status.success() {
            return Err("Failed to clone GitHub repository".to_string());
        }
    } else if source_type == "zip" {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(source_code)
            .map_err(|e| format!("Failed to decode base64 ZIP: {}", e))?;
        let zip_path = dest_dir.join("archive.zip");
        fs::write(&zip_path, &bytes).map_err(|e| e.to_string())?;
        
        let file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let outpath = match file.enclosed_name() {
                Some(path) => dest_dir.join(path),
                None => continue,
            };
            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            } else {
                if let Some(p) = outpath.parent() {
                    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                }
                let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
            }
        }
        let _ = fs::remove_file(zip_path);
    } else {
        let mut custom_cargo = false;
        let mut custom_main = false;

        if source_code.trim().starts_with('{') {
            if let Ok(files) = serde_json::from_str::<std::collections::HashMap<String, String>>(source_code) {
                for (path_str, content) in files {
                    let file_path = dest_dir.join(&path_str);
                    if let Some(parent) = file_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::write(&file_path, content);
                    if path_str == "Cargo.toml" {
                        custom_cargo = true;
                    }
                    if path_str == "src/main.rs" {
                        custom_main = true;
                    }
                }
            }
        }

        if !custom_cargo || !custom_main {
            let _ = fs::create_dir_all(dest_dir.join("src"));
        }

        if !custom_cargo {
            let cargo_toml = r#"[package]
name = "service"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
rust-sdk = { path = "./rust-sdk" }
"#;
            fs::write(dest_dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;
        }

        if !custom_main {
            fs::write(dest_dir.join("src/main.rs"), source_code).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

impl DockerManager {
    pub fn new() -> Result<Self, String> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| format!("Failed to connect to Docker: {}", e))?;
        Ok(Self { docker })
    }

    pub async fn build_image(
        &self,
        service_id: &str,
        version_tag: &str,
        source_type: &str,
        source_code: &str,
    ) -> Result<(String, String), String> {
        let cleaned_service_id = service_id.replace(":", "_").replace("-", "_").to_lowercase();
        let image_tag = format!("ms_{}:{}", cleaned_service_id, version_tag);

        let build_dir = PathBuf::from(format!("/tmp/build_{}_{}", cleaned_service_id, version_tag));
        if build_dir.exists() {
            let _ = fs::remove_dir_all(&build_dir);
        }
        fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;

        extract_source(source_type, source_code, &build_dir)?;

        let sdk_dest = build_dir.join("rust-sdk");
        fs::create_dir_all(sdk_dest.join("src")).map_err(|e| e.to_string())?;
        let sdk_cargo_concrete = r#"[package]
name = "rust-sdk"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "default-tls"] }
surrealdb = { version = "3.2.4", default-features = false, features = ["protocol-ws", "protocol-http", "native-tls"] }
"#;
        fs::write(sdk_dest.join("Cargo.toml"), sdk_cargo_concrete).map_err(|e| e.to_string())?;
        
        if let Ok(sdk_lib) = fs::read_to_string("/Users/welitonferreira/Workspaces/dooplo/doopack/sdks/rust-sdk/src/lib.rs") {
            let _ = fs::write(sdk_dest.join("src/lib.rs"), sdk_lib);
        }

        let dockerfile = r#"FROM rust:slim-bookworm as builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
ENV CARGO_BUILD_JOBS=1
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/service /usr/local/bin/service
CMD ["/usr/local/bin/service"]
"#;
        fs::write(build_dir.join("Dockerfile"), dockerfile).map_err(|e| e.to_string())?;

        // 4. Create tar archive of build context
        let tar_file_path = format!("/tmp/build_{}_{}.tar", cleaned_service_id, version_tag);
        let status = Command::new("tar")
            .args(&["-cf", &tar_file_path, "-C", build_dir.to_str().unwrap(), "."])
            .status()
            .map_err(|e| format!("Failed to run tar command: {}", e))?;

        if !status.success() {
            return Err("Failed to create tar archive".to_string());
        }

        // 5. Send tar archive to Docker build image endpoint
        let tar_content = fs::read(&tar_file_path).map_err(|e| e.to_string())?;
        
        let build_options = BuildImageOptions {
            t: image_tag.clone(),
            rm: true,
            forcerm: true,
            ..Default::default()
        };

        let log_file_path = format!("/tmp/build_log_{}.log", service_id);
        let _ = fs::write(&log_file_path, "");

        let mut build_logs = String::new();
        let mut build_stream = self.docker.build_image(build_options, None, Some(tar_content.into()));
        while let Some(msg) = build_stream.next().await {
            match msg {
                Ok(info) => {
                    if let Some(err) = info.error {
                        build_logs.push_str(&format!("\nERROR: {}", err));
                        let _ = fs::write(&log_file_path, &build_logs);
                        return Err(format!("Docker build error: {}\nLogs:\n{}", err, build_logs));
                    }
                    if let Some(stream) = info.stream {
                        print!("{}", stream);
                        build_logs.push_str(&stream);
                        let _ = fs::write(&log_file_path, &build_logs);
                    }
                }
                Err(e) => {
                    let _ = fs::write(&log_file_path, &build_logs);
                    return Err(format!("Docker API communication error: {}\nLogs:\n{}", e, build_logs));
                }
            }
        }

        // Cleanup
        let _ = fs::remove_dir_all(&build_dir);
        let _ = fs::remove_file(tar_file_path);

        Ok((image_tag, build_logs))
    }

    pub async fn run_container(
        &self,
        container_name: &str,
        image_tag: &str,
        payload_input: &serde_json::Value,
        env_vars: Option<std::collections::HashMap<String, String>>,
    ) -> Result<(String, serde_json::Value), String> {
        // Kill existing container if any
        let _ = self.stop_container(container_name).await;
        let remove_options = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });
        let _ = self.docker.remove_container(container_name, remove_options).await;
        
        // Pass payload input as an environment variable or via stdin
        let mut env_strings = vec![format!("PAYLOAD_INPUT={}", payload_input.to_string())];
        if let Some(vars) = env_vars {
            for (k, v) in vars {
                env_strings.push(format!("{}={}", k, v));
            }
        }
        let env_slices: Vec<&str> = env_strings.iter().map(|s| s.as_str()).collect();
        
        let config = Config {
            image: Some(image_tag),
            env: Some(env_slices),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let create_options = CreateContainerOptions {
            name: container_name,
            ..Default::default()
        };

        let container = self.docker.create_container(Some(create_options), config)
            .await
            .map_err(|e| format!("Failed to create container: {}", e))?;

        let id = container.id;

        // Wait for container to finish execution and retrieve logs/output
        let wait_options = Some(bollard::container::WaitContainerOptions {
            condition: "not-running",
        });
        let mut wait_stream = self.docker.wait_container(&id, wait_options);

        // Start container
        self.docker.start_container::<&str>(&id, None)
            .await
            .map_err(|e| format!("Failed to start container: {}", e))?;

        let wait_fut = wait_stream.next();
        let exit_status = match tokio::time::timeout(std::time::Duration::from_secs(15), wait_fut).await {
            Ok(Some(Ok(status))) => status,
            Ok(Some(Err(e))) => return Err(format!("Container wait failed: {}", e)),
            Ok(None) => return Err("Container wait stream ended early".to_string()),
            Err(_) => {
                // Fetch logs before removing the container
                let logs_options = Some(bollard::container::LogsOptions::<String> {
                    stdout: true,
                    stderr: true,
                    ..Default::default()
                });
                let mut logs_stream = self.docker.logs(&id, logs_options);
                let mut log_output = String::new();
                while let Some(Ok(log_chunk)) = logs_stream.next().await {
                    log_output.push_str(&log_chunk.to_string());
                }

                let _ = self.stop_container(container_name).await;
                let remove_options = Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                });
                let _ = self.docker.remove_container(container_name, remove_options).await;
                return Err(format!("Container execution timed out (15s). Logs so far: {}", log_output));
            }
        };

        {
            if exit_status.status_code != 0 {
                // Read logs for error output
                let logs_options = Some(bollard::container::LogsOptions::<String> {
                    stdout: true,
                    stderr: true,
                    ..Default::default()
                });
                let mut logs_stream = self.docker.logs(&id, logs_options);
                let mut log_output = String::new();
                while let Some(Ok(log_chunk)) = logs_stream.next().await {
                    log_output.push_str(&log_chunk.to_string());
                }
                
                // Cleanup container
                let remove_options = Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                });
                let _ = self.docker.remove_container(&id, remove_options).await;
                
                return Err(format!("Container exited with code {}. Logs: {}", exit_status.status_code, log_output));
            }
        }

        // Retrieve container execution output logs
        let logs_options = Some(bollard::container::LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..Default::default()
        });
        let mut logs_stream = self.docker.logs(&id, logs_options);
        let mut log_output = String::new();
        while let Some(Ok(log_chunk)) = logs_stream.next().await {
            log_output.push_str(&log_chunk.to_string());
        }

        // Remove container
        let remove_options = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });
        let _ = self.docker.remove_container(&id, remove_options).await;

        // Extract JSON response from stdout
        // Usually, the microservice writes its return value to stdout as JSON
        let trimmed_output = log_output.trim();
        let payload_output = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(trimmed_output) {
            json_val
        } else {
            serde_json::json!({ "raw_output": trimmed_output })
        };

        Ok((id, payload_output))
    }

    pub async fn stop_container(&self, container_id: &str) -> Result<(), String> {
        self.docker.stop_container(container_id, None)
            .await
            .map_err(|e| format!("Failed to stop container: {}", e))
    }

    pub async fn start_container(&self, container_id: &str) -> Result<(), String> {
        self.docker.start_container::<&str>(container_id, None)
            .await
            .map_err(|e| format!("Failed to start container: {}", e))
    }
}

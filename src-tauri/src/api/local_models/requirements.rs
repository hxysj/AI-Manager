use super::{failure, file_size, runtime, runtime_validation, AppPaths, ManagerError, ModelFile};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

const WRITE_RESERVE: u64 = 64 * 1024 * 1024;

pub(super) struct ModelRequirements {
    pub os: &'static str,
    pub arch: &'static str,
    pub platform_name: &'static str,
    pub memory_requirement: Option<MemoryRequirement>,
    pub files: &'static [ModelFile],
}

// 每个模型登记自身部署配置的内存条件；应用预算必须与已核实的硬性要求区分。
pub(super) struct MemoryRequirement {
    pub available_bytes: u64,
    pub estimated: bool,
    pub source: &'static str,
}

#[derive(Serialize)]
pub(super) struct Check {
    label: &'static str,
    passed: Option<bool>,
    detail: String,
}

#[derive(Serialize)]
pub(super) struct Compatibility {
    allowed: bool,
    checks: Vec<Check>,
    validation: Value,
}

impl Compatibility {
    pub(super) fn ensure_allowed(&self) -> Result<(), ManagerError> {
        if self.allowed {
            return Ok(());
        }
        Err(failure(format!(
            "本地模型使用条件未满足：{}",
            self.checks
                .iter()
                .filter(|check| check.passed != Some(true))
                .map(|check| check.detail.as_str())
                .collect::<Vec<_>>()
                .join("；")
        )))
    }

    pub(super) fn ensure_runtime_prerequisites(&self) -> Result<(), ManagerError> {
        // 实测允许尝试未知的内存条件，但不跳过已知不满足的系统、内存或磁盘要求。
        let failures: Vec<_> = self
            .checks
            .iter()
            .filter(|check| check.passed == Some(false))
            .map(|check| check.detail.as_str())
            .collect();
        if failures.is_empty() {
            return Ok(());
        }
        Err(failure(failures.join("；")))
    }
}

struct Resources {
    os: &'static str,
    arch: &'static str,
    memory: Result<(u64, u64), String>,
    disk: Result<u64, String>,
}

fn gib(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn evaluate(
    resources: Resources,
    requirements: &ModelRequirements,
    required_disk: u64,
    for_download: bool,
) -> Compatibility {
    let supported = resources.os == requirements.os && resources.arch == requirements.arch;
    let memory = match (resources.memory, &requirements.memory_requirement) {
        (Ok((total, available)), Some(minimum)) => Check {
            label: "可用内存",
            // 预估预算只控制下载准入；已下载模型保留实际校验入口，硬性要求仍需满足。
            passed: Some(
                available >= minimum.available_bytes || (!for_download && minimum.estimated),
            ),
            detail: format!(
                "{} {}，当前可用 {}（总计 {}）；依据：{}{}",
                if minimum.estimated {
                    "应用预估内存预算（非官方最低要求）"
                } else {
                    "已核实最低可用内存"
                },
                gib(minimum.available_bytes),
                gib(available),
                gib(total),
                minimum.source,
                if available < minimum.available_bytes {
                    if total < minimum.available_bytes {
                        "；设备总内存不足预算"
                    } else {
                        "；当前可用内存不足，请释放内存后刷新检测"
                    }
                } else {
                    ""
                }
            ),
        },
        (Ok((total, available)), None) => Check {
            label: "可用内存",
            passed: None,
            detail: format!(
                "当前可用 {}（总计 {}）；此模型尚未配置下载前内存预算",
                gib(available),
                gib(total)
            ),
        },
        (Err(error), minimum) => Check {
            label: "可用内存",
            passed: minimum.as_ref().map(|_| false),
            detail: format!("无法检测物理内存：{error}；运行能力尚未确认"),
        },
    };
    let disk = match resources.disk {
        Ok(available) => Check {
            label: "存储空间",
            passed: Some(available >= required_disk),
            detail: format!(
                "模型存储盘还需 {}，当前可用 {}",
                gib(required_disk),
                gib(available)
            ),
        },
        Err(error) => Check {
            label: "存储空间",
            passed: Some(false),
            detail: format!("无法检测模型存储盘空间：{error}"),
        },
    };
    let checks = vec![
        Check {
            label: "系统与架构",
            passed: Some(supported),
            detail: if supported {
                requirements.platform_name.into()
            } else {
                format!(
                    "当前为 {} / {}，此模型要求 {}",
                    resources.os, resources.arch, requirements.platform_name
                )
            },
        },
        memory,
        disk,
    ];
    Compatibility {
        // 下载准入只依赖模型资源条件，不依赖必须先下载才能获得的运行记录。
        allowed: checks.iter().all(|check| check.passed == Some(true)),
        checks,
        validation: json!({}),
    }
}

fn remaining_model_bytes(directory: &Path, files: &[ModelFile]) -> Result<u64, ManagerError> {
    let mut remaining = 0;
    for file in files {
        let target = directory.join(file.name);
        if file_size(&target)? == file.bytes {
            continue;
        }
        let cached = file_size(&target.with_extension("gguf.part"))?;
        // 超出清单大小的缓存会被下载器丢弃，不能计入可续传字节。
        remaining += file.bytes - if cached <= file.bytes { cached } else { 0 };
    }
    Ok(remaining)
}

pub(super) fn inspect(
    paths: &AppPaths,
    directory: &Path,
    requirements: &ModelRequirements,
    for_download: bool,
) -> Result<Compatibility, ManagerError> {
    let model_bytes = if for_download {
        remaining_model_bytes(directory, requirements.files)?
    } else {
        0
    };
    let required = model_bytes + runtime::required_disk_bytes(paths)?;
    let mut report = evaluate(
        Resources {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            memory: memory_status(),
            disk: disk_available(directory),
        },
        requirements,
        if required > 0 {
            required + WRITE_RESERVE
        } else {
            0
        },
        for_download,
    );
    if for_download {
        report.validation = runtime_validation(directory)?;
    }
    Ok(report)
}

#[cfg(windows)]
pub(super) fn memory_status() -> Result<(u64, u64), String> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok((status.ullTotalPhys, status.ullAvailPhys))
}

#[cfg(windows)]
fn disk_available(directory: &Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    // 尚未创建模型目录时，查询最近的已有父目录，确保检测的是实际存储盘。
    let existing = directory
        .ancestors()
        .find(|path| path.is_dir())
        .ok_or_else(|| "模型存储目录不可访问".to_string())?;
    let mut path: Vec<u16> = existing.as_os_str().encode_wide().collect();
    if !path.ends_with(&[b'\\' as u16]) {
        path.push(b'\\' as u16);
    }
    path.push(0);
    let mut available = 0;
    if unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(available)
}

#[cfg(not(windows))]
pub(super) fn memory_status() -> Result<(u64, u64), String> {
    Err("当前系统暂不支持本地推理".into())
}

#[cfg(not(windows))]
fn disk_available(_: &Path) -> Result<u64, String> {
    Err("当前系统暂不支持本地推理".into())
}

#[cfg(test)]
mod tests {
    use super::super::{FILES, MODEL_REQUIREMENTS};
    use super::*;

    fn resources() -> Resources {
        Resources {
            os: "windows",
            arch: "x86_64",
            memory: Ok((16 * 1024 * 1024 * 1024, 3 * 1024 * 1024 * 1024)),
            disk: Ok(2 * 1024 * 1024 * 1024),
        }
    }

    #[test]
    fn checks_platform_disk_and_exact_boundaries() {
        let required = 2 * 1024 * 1024 * 1024;
        assert!(evaluate(resources(), &MODEL_REQUIREMENTS, required, true)
            .ensure_allowed()
            .is_ok());
        let mut unsupported = resources();
        unsupported.arch = "aarch64";
        assert!(evaluate(unsupported, &MODEL_REQUIREMENTS, required, true)
            .ensure_allowed()
            .unwrap_err()
            .to_string()
            .contains("此模型要求 Windows x64"));
        assert!(!evaluate(resources(), &MODEL_REQUIREMENTS, required + 1, true).allowed);
    }

    #[test]
    fn each_model_uses_its_own_memory_and_platform_requirements() {
        let demanding = ModelRequirements {
            memory_requirement: Some(MemoryRequirement {
                available_bytes: 4 * 1024 * 1024 * 1024,
                estimated: false,
                source: "测试模型的已核实运行条件",
            }),
            ..MODEL_REQUIREMENTS
        };
        assert!(evaluate(resources(), &MODEL_REQUIREMENTS, 0, true).allowed);
        let report = evaluate(resources(), &demanding, 0, true);
        assert!(!report.allowed);
        assert_eq!(report.checks[1].passed, Some(false));
        assert!(report
            .ensure_allowed()
            .unwrap_err()
            .to_string()
            .contains("测试模型的已核实运行条件"));
        let mut enough_memory = resources();
        enough_memory.memory.as_mut().unwrap().1 = 4 * 1024 * 1024 * 1024;
        assert!(evaluate(enough_memory, &demanding, 0, true).allowed);
        let mut below_minimum = resources();
        below_minimum.memory.as_mut().unwrap().1 = 4 * 1024 * 1024 * 1024 - 1;
        assert!(!evaluate(below_minimum, &demanding, 0, true).allowed);
        let mut below_minimum = resources();
        below_minimum.memory.as_mut().unwrap().1 = 4 * 1024 * 1024 * 1024 - 1;
        assert!(evaluate(below_minimum, &demanding, 0, false)
            .ensure_runtime_prerequisites()
            .is_err());
        let mut unknown_memory = resources();
        unknown_memory.memory = Err("检测失败".into());
        assert!(!evaluate(unknown_memory, &demanding, 0, true).allowed);
        let other_platform = ModelRequirements {
            os: "linux",
            platform_name: "Linux x64",
            ..MODEL_REQUIREMENTS
        };
        assert!(!evaluate(resources(), &other_platform, 0, true).allowed);
    }

    #[test]
    fn unknown_resources_block_download_and_recovery_is_allowed() {
        let mut unknown = resources();
        unknown.memory = Err("检测失败".into());
        unknown.disk = Err("磁盘不可访问".into());
        let message = evaluate(unknown, &MODEL_REQUIREMENTS, 1, true)
            .ensure_allowed()
            .unwrap_err()
            .to_string();
        assert!(message.contains("无法检测物理内存"));
        assert!(message.contains("无法检测模型存储盘空间"));
        assert!(evaluate(resources(), &MODEL_REQUIREMENTS, 1, false)
            .ensure_runtime_prerequisites()
            .is_ok());
    }

    #[test]
    fn first_download_uses_catalog_budget_without_weights_or_validation() {
        let root = super::super::tests::Fixture::new();
        let remaining = remaining_model_bytes(&root.0, &FILES).unwrap();
        assert_eq!(remaining, FILES.iter().map(|file| file.bytes).sum::<u64>());
        assert!(super::super::runtime_validation(&root.0).unwrap()["passed"].is_null());
        let report = evaluate(resources(), &MODEL_REQUIREMENTS, remaining, true);
        assert!(report.ensure_allowed().is_ok());
        assert!(report.checks[1].detail.contains("应用预估内存预算"));
        assert!(report.checks[1].detail.contains("非官方最低要求"));
    }

    #[test]
    fn old_validation_does_not_replace_current_resource_checks() {
        let mut report = evaluate(resources(), &MODEL_REQUIREMENTS, 1, true);
        report.validation = json!({"passed": false, "detail": "上次识别失败"});
        assert!(report.ensure_allowed().is_ok(), "旧失败不构成首次下载条件");
        let mut low_memory = resources();
        low_memory.memory.as_mut().unwrap().1 = 2_598_455_214;
        let mut report = evaluate(low_memory, &MODEL_REQUIREMENTS, 1, true);
        report.validation = json!({"passed": true, "detail": "上次校验通过"});
        assert!(report.ensure_allowed().is_err(), "旧成功不能覆盖当前资源不足");
        assert!(report.checks[1].detail.contains("2.42 GiB"));
        assert!(report.checks[1].detail.contains("请释放内存后刷新检测"));
        let mut low_memory = resources();
        low_memory.memory.as_mut().unwrap().1 = 2_598_455_214;
        assert!(evaluate(low_memory, &MODEL_REQUIREMENTS, 1, false)
            .ensure_runtime_prerequisites()
            .is_ok(), "已下载模型允许通过实测确认预估预算以下的运行能力");
        let unconfigured = ModelRequirements {
            memory_requirement: None,
            ..MODEL_REQUIREMENTS
        };
        let report = evaluate(resources(), &unconfigured, 1, true);
        assert!(!report.allowed);
        assert!(report.checks[1].detail.contains("尚未配置下载前内存预算"));
        let mut unknown_memory = resources();
        unknown_memory.memory = Err("检测失败".into());
        let report = evaluate(unknown_memory, &MODEL_REQUIREMENTS, 1, true);
        assert!(!report.allowed);
        assert_eq!(report.checks[1].passed, Some(false));
        assert!(report.checks[1].detail.contains("无法检测物理内存"));
    }

    #[test]
    fn installed_runtime_does_not_require_additional_disk_space() {
        let mut full_disk = resources();
        full_disk.disk = Ok(0);
        assert!(evaluate(full_disk, &MODEL_REQUIREMENTS, 0, false)
            .ensure_runtime_prerequisites()
            .is_ok());
    }

    #[test]
    fn remaining_bytes_match_download_resume_and_invalid_cache() {
        let root = super::super::tests::Fixture::new();
        let total: u64 = FILES.iter().map(|file| file.bytes).sum();
        assert_eq!(remaining_model_bytes(&root.0, &FILES).unwrap(), total);
        let target = root.0.join(FILES[0].name);
        let partial = target.with_extension("gguf.part");
        let file = std::fs::File::create(&partial).unwrap();
        file.set_len(123).unwrap();
        assert_eq!(remaining_model_bytes(&root.0, &FILES).unwrap(), total - 123);
        file.set_len(FILES[0].bytes + 1).unwrap();
        assert_eq!(remaining_model_bytes(&root.0, &FILES).unwrap(), total);
        file.set_len(FILES[0].bytes).unwrap();
        assert_eq!(
            remaining_model_bytes(&root.0, &FILES).unwrap(),
            FILES[1].bytes
        );
        std::fs::File::create(target)
            .unwrap()
            .set_len(FILES[0].bytes)
            .unwrap();
        assert_eq!(
            remaining_model_bytes(&root.0, &FILES).unwrap(),
            FILES[1].bytes
        );
        drop(file);
    }

    #[cfg(windows)]
    #[test]
    fn detects_memory_and_storage_before_model_directory_exists() {
        let root = super::super::tests::Fixture::new();
        let (total, available) = memory_status().unwrap();
        assert!(total > 0 && available <= total);
        assert!(disk_available(&root.0.join("missing").join("models")).is_ok());
    }
}

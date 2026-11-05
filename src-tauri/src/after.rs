//! 批次结束后的动作：打开输出目录 / 关机。
//!
//! 这一层只负责**构造命令**与（可选地）执行；关机默认不执行，需要显式环境变量
//! `FORMATFORGE_ALLOW_SHUTDOWN=1`，避免测试或误设置真的关掉开发机。
//! 单元测试只做命令构造断言，绝不调用 `execute`。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub fn open_folder_command(dir: &str) -> CommandSpec {
    #[cfg(windows)]
    {
        CommandSpec {
            program: "explorer".into(),
            args: vec![dir.to_string()],
        }
    }
    #[cfg(not(windows))]
    {
        CommandSpec {
            program: "xdg-open".into(),
            args: vec![dir.to_string()],
        }
    }
}

/// 关机命令：Windows 用 `shutdown /s /t <秒>`；其它平台用 `shutdown -h +<分钟>`
/// （POSIX 的 `+m` 单位是分钟，这里向上取整，避免 0 分钟立即关机）。
pub fn shutdown_command(delay_secs: u64) -> CommandSpec {
    #[cfg(windows)]
    {
        CommandSpec {
            program: "shutdown".into(),
            args: vec!["/s".into(), "/t".into(), delay_secs.to_string()],
        }
    }
    #[cfg(not(windows))]
    {
        let minutes = delay_secs.div_ceil(60).max(1);
        CommandSpec {
            program: "shutdown".into(),
            args: vec!["-h".into(), format!("+{minutes}")],
        }
    }
}

/// 只有显式设置该环境变量时才允许真正下发关机命令。
pub fn shutdown_enabled() -> bool {
    std::env::var("FORMATFORGE_ALLOW_SHUTDOWN")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

pub fn execute(spec: &CommandSpec) -> Result<(), String> {
    let mut command = std::process::Command::new(&spec.program);
    command.args(&spec.args);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动 {} 失败：{e}", spec.program))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_folder_command_points_at_output_dir() {
        let spec = open_folder_command(r"D:\out");
        #[cfg(windows)]
        {
            assert_eq!(spec.program, "explorer");
            assert_eq!(spec.args, vec![r"D:\out".to_string()]);
        }
        #[cfg(not(windows))]
        {
            assert_eq!(spec.program, "xdg-open");
            assert_eq!(spec.args, vec![r"D:\out".to_string()]);
        }
    }

    #[test]
    fn shutdown_command_is_constructed_but_never_executed() {
        let spec = shutdown_command(60);
        assert_eq!(spec.program, "shutdown");
        #[cfg(windows)]
        {
            assert_eq!(spec.args[0], "/s");
            assert!(spec.args.contains(&"/t".to_string()));
            assert!(spec.args.contains(&"60".to_string()));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(spec.args, vec!["-h".to_string(), "+1".to_string()]);
        }
        // 这条测试只断言参数，故意不调用 execute：测试绝不允许真正关机。
    }

    #[test]
    fn shutdown_requires_explicit_opt_in() {
        // 默认环境下开关必须是关闭的。
        std::env::remove_var("FORMATFORGE_ALLOW_SHUTDOWN");
        assert!(!shutdown_enabled());
        std::env::set_var("FORMATFORGE_ALLOW_SHUTDOWN", "1");
        assert!(shutdown_enabled());
        std::env::remove_var("FORMATFORGE_ALLOW_SHUTDOWN");
    }
}

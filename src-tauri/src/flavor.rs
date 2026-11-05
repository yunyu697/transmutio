//! 发行变体（flavor）：LGPL 默认包 与 GPL 变体。
//!
//! 变体在**编译期**确定（Cargo feature `gpl`），同一份源码、不复制分支。
//! GPL 变体的 `allow_gpl` 固定为 true，运行时设置与环境变量都不能关闭它。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    Lgpl,
    Gpl,
}

/// GPL 变体：`cargo build --features gpl`（构建脚本 `npm run build:gpl` 会加上）。
pub const IS_GPL: bool = cfg!(feature = "gpl");

/// 由 build.rs 注入的编译期变体字符串，供产物自检（flavor-report / 构建脚本核对）。
pub const BUILD_FLAVOR: &str = env!("FF_FLAVOR");

impl Flavor {
    pub const fn current() -> Self {
        if IS_GPL {
            Self::Gpl
        } else {
            Self::Lgpl
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Lgpl => "lgpl",
            Self::Gpl => "gpl",
        }
    }

    pub const fn product_name(self) -> &'static str {
        match self {
            Self::Lgpl => "Transmutio",
            Self::Gpl => "Transmutio GPL",
        }
    }

    /// 安装包 / 应用数据目录的标识，两个变体必须不同，避免共享 tools 与配置。
    pub const fn app_identifier(self) -> &'static str {
        match self {
            Self::Lgpl => "app.transmutio.desktop",
            Self::Gpl => "app.transmutio.desktop.gpl",
        }
    }

    /// 旧版（FormatForge）的 identifier：仅供数据迁移与旧版并存拦截使用。
    pub const fn legacy_app_identifier(self) -> &'static str {
        match self {
            Self::Lgpl => "app.formatforge.desktop",
            Self::Gpl => "app.formatforge.desktop.gpl",
        }
    }

    /// 更新通道隔离，避免 GPL 用户被 LGPL 更新覆盖（反之亦然）。
    pub const fn update_channel(self) -> &'static str {
        match self {
            Self::Lgpl => "transmutio-lgpl-stable",
            Self::Gpl => "transmutio-gpl-stable",
        }
    }

    pub const fn default_source_url(self) -> &'static str {
        match self {
            Self::Lgpl => "https://ffmpeg.org/download.html",
            Self::Gpl => "https://www.gyan.dev/ffmpeg/builds/",
        }
    }

    pub const fn license_label(self) -> &'static str {
        match self {
            Self::Lgpl => "LGPL-2.1",
            Self::Gpl => "GPL-2.0-or-later（以所选 FFmpeg 构建为准）",
        }
    }

    /// GPL 变体编译期固定 true；LGPL 版只有 debug 构建允许本机开发覆盖。
    pub const fn allow_gpl(self, dev_override: bool) -> bool {
        match self {
            Self::Gpl => true,
            Self::Lgpl => cfg!(debug_assertions) && dev_override,
        }
    }

    /// GPL 变体要求 FFmpeg 本身也是 GPL 构建（保证 x264/x265 可用）。
    pub const fn require_gpl_build(self) -> bool {
        matches!(self, Self::Gpl)
    }

    pub const fn dev_override_available(self) -> bool {
        match self {
            Self::Lgpl => cfg!(debug_assertions),
            Self::Gpl => false,
        }
    }

    /// 变体对 FFmpeg 构建的接受规则。`dev_override` 只是 LGPL debug 的开发开关。
    pub fn accepts_build(self, gpl: bool, nonfree: bool, dev_override: bool) -> Result<(), String> {
        if nonfree {
            return Err("该构建包含 nonfree 组件（如 libfdk-aac），任何变体都不接受。".into());
        }
        match self {
            Self::Gpl => {
                if gpl {
                    Ok(())
                } else {
                    Err("当前是 GPL 版（Transmutio GPL）：需要 GPL 构建的 FFmpeg（含 libx264/libx265）。\
                         所选 LGPL 构建在 GPL 版中不可用。"
                        .into())
                }
            }
            Self::Lgpl => {
                if gpl && !self.allow_gpl(dev_override) {
                    Err("当前是 LGPL 版：不接受 GPL 构建的 FFmpeg。如需 x264/x265 软件编码，\
                         请改用 GPL 版（Transmutio GPL）。"
                        .into())
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn info(self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id(),
            "productName": self.product_name(),
            "appIdentifier": self.app_identifier(),
            "legacyAppIdentifier": self.legacy_app_identifier(),
            "updateChannel": self.update_channel(),
            "defaultSourceUrl": self.default_source_url(),
            "licenseLabel": self.license_label(),
            "allowGpl": self.allow_gpl(false),
            "allowGplCompiled": IS_GPL,
            "requireGplBuild": self.require_gpl_build(),
            "devOverrideAvailable": self.dev_override_available(),
            "debug": cfg!(debug_assertions),
        })
    }
}

/// 运行时生效的 GPL 策略：完全由编译期变体决定，没有任何环境变量后门。
/// LGPL 版的开发开关只存在于 debug 构建，release 构建下设置项同样无效。
pub fn allow_gpl_effective(dev_override: bool) -> bool {
    Flavor::current().allow_gpl(dev_override)
}

pub fn report_json() -> String {
    serde_json::json!({
        "flavor": Flavor::current().id(),
        "buildFlavor": BUILD_FLAVOR,
        "allowGpl": Flavor::current().allow_gpl(false),
        "requireGplBuild": Flavor::current().require_gpl_build(),
        "productName": Flavor::current().product_name(),
        "appIdentifier": Flavor::current().app_identifier(),
        "legacyAppIdentifier": Flavor::current().legacy_app_identifier(),
        "updateChannel": Flavor::current().update_channel(),
        "debug": cfg!(debug_assertions),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpl_variant_is_fixed_on() {
        assert!(Flavor::Gpl.allow_gpl(false));
        assert!(Flavor::Gpl.allow_gpl(true));
    }

    #[test]
    fn lgpl_variant_rejects_gpl_by_default() {
        // 生产行为：不接受 GPL；debug 构建可通过显式开发开关放行。
        assert!(!Flavor::Lgpl.allow_gpl(false));
        assert!(Flavor::Lgpl.accepts_build(true, false, false).is_err());
        assert!(Flavor::Lgpl.accepts_build(false, false, false).is_ok());
        assert!(Flavor::Lgpl.accepts_build(false, true, true).is_err());
    }

    #[test]
    fn gpl_variant_requires_gpl_build() {
        assert!(Flavor::Gpl.accepts_build(true, false, false).is_ok());
        assert!(Flavor::Gpl.accepts_build(false, false, true).is_err());
        assert!(Flavor::Gpl.accepts_build(true, true, true).is_err());
    }

    #[test]
    fn variants_have_distinct_identity() {
        assert_eq!(Flavor::Lgpl.app_identifier(), "app.transmutio.desktop");
        assert_eq!(Flavor::Gpl.app_identifier(), "app.transmutio.desktop.gpl");
        assert_eq!(Flavor::Lgpl.legacy_app_identifier(), "app.formatforge.desktop");
        assert_eq!(Flavor::Gpl.legacy_app_identifier(), "app.formatforge.desktop.gpl");
        assert_eq!(Flavor::Lgpl.update_channel(), "transmutio-lgpl-stable");
        assert_eq!(Flavor::Gpl.update_channel(), "transmutio-gpl-stable");
        assert_ne!(Flavor::Lgpl.app_identifier(), Flavor::Gpl.app_identifier());
        assert_ne!(Flavor::Lgpl.product_name(), Flavor::Gpl.product_name());
        assert_ne!(Flavor::Lgpl.update_channel(), Flavor::Gpl.update_channel());
        assert_ne!(Flavor::Lgpl.default_source_url(), Flavor::Gpl.default_source_url());
    }

    /// 设置项/环境变量都翻转不了 LGPL 版：allowGpl 由编译期固定。
    #[test]
    fn runtime_config_cannot_turn_on_gpl_for_lgpl() {
        std::env::set_var("FORMATFORGE_ALLOW_GPL", "1");
        let err = Flavor::Lgpl
            .accepts_build(true, false, false)
            .expect_err("LGPL 版在无显式开发开关时必须拒绝 GPL 构建");
        assert!(err.contains("GPL 版"), "拒绝文案要指向 GPL 版：{err}");
        assert!(!Flavor::Lgpl.allow_gpl(false));
        if !cfg!(debug_assertions) {
            // release 构建没有开发开关：设置项打开也不能放行。
            assert!(!Flavor::Lgpl.allow_gpl(true), "release 构建不允许设置项放行 GPL");
            assert!(Flavor::Lgpl.accepts_build(true, false, true).is_err());
        }
        std::env::remove_var("FORMATFORGE_ALLOW_GPL");
    }

    /// GPL 变体的 allowGpl 同样由编译期固定，运行时无法关闭。
    #[test]
    fn runtime_config_cannot_turn_off_gpl_for_gpl() {
        std::env::set_var("FORMATFORGE_ALLOW_GPL", "0");
        std::env::set_var("FORMATFORGE_DISABLE_GPL", "1");
        assert!(Flavor::Gpl.allow_gpl(false), "GPL 变体 allowGpl 恒为 true");
        assert!(Flavor::Gpl.accepts_build(true, false, false).is_ok());
        assert!(!Flavor::Gpl.dev_override_available());
        std::env::remove_var("FORMATFORGE_ALLOW_GPL");
        std::env::remove_var("FORMATFORGE_DISABLE_GPL");
    }

    #[test]
    fn lgpl_dev_override_only_in_debug() {
        assert_eq!(Flavor::Lgpl.dev_override_available(), cfg!(debug_assertions));
        if cfg!(debug_assertions) {
            assert!(Flavor::Lgpl.accepts_build(true, false, true).is_ok());
        } else {
            assert!(Flavor::Lgpl.accepts_build(true, false, true).is_err());
        }
    }
}

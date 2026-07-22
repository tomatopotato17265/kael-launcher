//! Theseus settings file

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;

// Types
/// Global Theseus settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub max_concurrent_downloads: usize,
    pub max_concurrent_writes: usize,

    pub locale: String,
    pub default_page: DefaultPage,
    pub collapsed_navigation: bool,
    pub hide_nametag_skins_page: bool,
    pub advanced_rendering: bool,
    pub native_decorations: bool,
    pub toggle_sidebar: bool,

    /// Primary theme config: hex seed driving the surface-shade ramp and
    /// base-mode (light/dark) selection. Used as-is when
    /// `sync_theme_with_system` is false, or as the "light" variant when true.
    pub color_theme: String,
    pub brand_color: String,
    /// Id of the selected preset/installed theme, or `None` for custom hex values.
    pub active_theme_preset: Option<String>,
    /// Custom themes folder. `None` means the default `config_dir/themes`.
    pub theme_dir: Option<String>,

    /// Dark-variant theme config, only meaningful when `sync_theme_with_system` is true.
    pub sync_theme_with_system: bool,
    pub dark_color_theme: String,
    pub dark_brand_color: String,
    pub dark_active_theme_preset: Option<String>,

    /// Id of the selected installed font, or `None`/`"default"` for the built-in font.
    pub active_font: Option<String>,
    /// Custom fonts folder. `None` means the default `config_dir/fonts`.
    pub font_dir: Option<String>,

    pub telemetry: bool,
    pub discord_rpc: bool,
    pub personalized_ads: bool,

    pub extra_launch_args: Vec<String>,
    pub custom_env_vars: Vec<(String, String)>,
    pub memory: MemorySettings,
    pub force_fullscreen: bool,
    pub game_resolution: WindowSize,
    pub hide_on_process_start: bool,
    pub hooks: Hooks,

    pub custom_dir: Option<String>,
    pub prev_custom_dir: Option<String>,
    pub migrated: bool,

    pub developer_mode: bool,
    pub feature_flags: HashMap<FeatureFlag, bool>,

    pub skipped_update: Option<String>,
    pub pending_update_toast_for_version: Option<String>,
    pub auto_download_updates: Option<bool>,

    pub version: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureFlag {
    PagePath,
    ProjectBackground,
    WorldsTab,
    WorldsInHome,
    ServerRamAsBytesAlwaysOn,
    AlwaysShowAppControls,
    SkipUnknownPackWarning,
    PrideFundraiser,
    ServersInApp,
    ServerProjectQa,
    I18nDebug,
    ShowInstancePlayTime,
}

impl Settings {
    const CURRENT_VERSION: usize = 5;

    pub async fn get(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<Self> {
        let res = sqlx::query!(
            r#"
            SELECT
                max_concurrent_writes, max_concurrent_downloads,
                locale, default_page, collapsed_navigation, hide_nametag_skins_page, advanced_rendering, native_decorations,
                discord_rpc, developer_mode, telemetry, personalized_ads,
                json(extra_launch_args) extra_launch_args, json(custom_env_vars) custom_env_vars,
                mc_memory_max, mc_force_fullscreen, mc_game_resolution_x, mc_game_resolution_y, hide_on_process_start,
                hook_pre_launch, hook_wrapper, hook_post_exit,
                custom_dir, prev_custom_dir, migrated, json(feature_flags) feature_flags, toggle_sidebar,
                skipped_update, pending_update_toast_for_version, auto_download_updates,
                brand_color, color_theme, dark_color_theme, dark_brand_color,
                sync_theme_with_system, active_theme_preset, dark_active_theme_preset, theme_dir,
                active_font, font_dir,
                version
            FROM settings
            "#
        )
            .fetch_one(exec)
            .await?;

        Ok(Self {
            max_concurrent_downloads: res.max_concurrent_downloads as usize,
            max_concurrent_writes: res.max_concurrent_writes as usize,
            locale: res.locale,
            default_page: DefaultPage::from_string(&res.default_page),
            collapsed_navigation: res.collapsed_navigation == 1,
            hide_nametag_skins_page: res.hide_nametag_skins_page == 1,
            advanced_rendering: res.advanced_rendering == 1,
            native_decorations: res.native_decorations == 1,
            toggle_sidebar: res.toggle_sidebar == 1,
            telemetry: res.telemetry == 1,
            discord_rpc: res.discord_rpc == 1,
            developer_mode: res.developer_mode == 1,
            personalized_ads: res.personalized_ads == 1,
            extra_launch_args: res
                .extra_launch_args
                .as_ref()
                .and_then(|x| serde_json::from_str(x).ok())
                .unwrap_or_default(),
            custom_env_vars: res
                .custom_env_vars
                .as_ref()
                .and_then(|x| serde_json::from_str(x).ok())
                .unwrap_or_default(),
            memory: MemorySettings {
                maximum: res.mc_memory_max as u32,
            },
            force_fullscreen: res.mc_force_fullscreen == 1,
            game_resolution: WindowSize(
                res.mc_game_resolution_x as u16,
                res.mc_game_resolution_y as u16,
            ),
            hide_on_process_start: res.hide_on_process_start == 1,
            hooks: Hooks {
                pre_launch: res.hook_pre_launch,
                wrapper: res.hook_wrapper,
                post_exit: res.hook_post_exit,
            },
            custom_dir: res.custom_dir,
            prev_custom_dir: res.prev_custom_dir,
            migrated: res.migrated == 1,
            feature_flags: res
                .feature_flags
                .as_ref()
                .and_then(|x| serde_json::from_str(x).ok())
                .unwrap_or_default(),
            skipped_update: res.skipped_update,
            pending_update_toast_for_version: res
                .pending_update_toast_for_version,
            auto_download_updates: res.auto_download_updates.map(|x| x == 1),
            brand_color: res
                .brand_color
                .unwrap_or_else(|| "#874EFE".to_string()),
            color_theme: res.color_theme,
            active_theme_preset: res.active_theme_preset,
            theme_dir: res.theme_dir,
            sync_theme_with_system: res.sync_theme_with_system == 1,
            dark_color_theme: res.dark_color_theme,
            dark_brand_color: res.dark_brand_color,
            dark_active_theme_preset: res.dark_active_theme_preset,
            active_font: res.active_font,
            font_dir: res.font_dir,
            version: res.version as usize,
        })
    }

    pub async fn update(
        &self,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        let max_concurrent_writes = self.max_concurrent_writes as i32;
        let max_concurrent_downloads = self.max_concurrent_downloads as i32;
        let default_page = self.default_page.as_str();
        let extra_launch_args = serde_json::to_string(&self.extra_launch_args)?;
        let custom_env_vars = serde_json::to_string(&self.custom_env_vars)?;
        let feature_flags = serde_json::to_string(&self.feature_flags)?;
        let version = self.version as i64;

        sqlx::query!(
            "
            UPDATE settings
            SET
                max_concurrent_writes = $1,
                max_concurrent_downloads = $2,

                locale = $3,
                default_page = $4,
                collapsed_navigation = $5,
                advanced_rendering = $6,
                native_decorations = $7,

                discord_rpc = $8,
                developer_mode = $9,
                telemetry = $10,
                personalized_ads = $11,

                extra_launch_args = jsonb($12),
                custom_env_vars = jsonb($13),
                mc_memory_max = $14,
                mc_force_fullscreen = $15,
                mc_game_resolution_x = $16,
                mc_game_resolution_y = $17,
                hide_on_process_start = $18,

                hook_pre_launch = $19,
                hook_wrapper = $20,
                hook_post_exit = $21,

                custom_dir = $22,
                prev_custom_dir = $23,
                migrated = $24,

                toggle_sidebar = $25,
                feature_flags = $26,
                hide_nametag_skins_page = $27,

                skipped_update = $28,
                pending_update_toast_for_version = $29,
                auto_download_updates = $30,

                brand_color = $31,
                color_theme = $32,
                dark_color_theme = $33,
                dark_brand_color = $34,
                sync_theme_with_system = $35,
                active_theme_preset = $36,
                dark_active_theme_preset = $37,
                theme_dir = $38,
                active_font = $39,
                font_dir = $40,

                version = $41
            ",
            max_concurrent_writes,
            max_concurrent_downloads,
            self.locale,
            default_page,
            self.collapsed_navigation,
            self.advanced_rendering,
            self.native_decorations,
            self.discord_rpc,
            self.developer_mode,
            self.telemetry,
            self.personalized_ads,
            extra_launch_args,
            custom_env_vars,
            self.memory.maximum,
            self.force_fullscreen,
            self.game_resolution.0,
            self.game_resolution.1,
            self.hide_on_process_start,
            self.hooks.pre_launch,
            self.hooks.wrapper,
            self.hooks.post_exit,
            self.custom_dir,
            self.prev_custom_dir,
            self.migrated,
            self.toggle_sidebar,
            feature_flags,
            self.hide_nametag_skins_page,
            self.skipped_update,
            self.pending_update_toast_for_version,
            self.auto_download_updates,
            self.brand_color,
            self.color_theme,
            self.dark_color_theme,
            self.dark_brand_color,
            self.sync_theme_with_system,
            self.active_theme_preset,
            self.dark_active_theme_preset,
            self.theme_dir,
            self.active_font,
            self.font_dir,
            version,
        )
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn migrate(exec: &Pool<Sqlite>) -> crate::Result<()> {
        let mut settings = Self::get(exec).await?;

        if settings.version < Settings::CURRENT_VERSION {
            tracing::info!(
                "Migrating settings version {} to {:?}",
                settings.version,
                Settings::CURRENT_VERSION
            );
        }

        // The legacy `theme` column (Dark/Light/Oled/System) is no longer part
        // of `Settings` itself, but is still needed to translate existing
        // users onto the new hex-based color_theme during the version 4 -> 5
        // migration below, so it's fetched separately here.
        let legacy_theme: Option<String> =
            sqlx::query_scalar!("SELECT theme FROM settings WHERE id = 0")
                .fetch_optional(exec)
                .await?;

        while settings.version < Settings::CURRENT_VERSION {
            if let Err(err) = settings.perform_migration(legacy_theme.as_deref())
            {
                tracing::error!(
                    "Failed to migrate settings from version {}: {}",
                    settings.version,
                    err
                );
                return Err(err);
            }
        }

        settings.update(exec).await?;

        Ok(())
    }

    pub fn perform_migration(
        &mut self,
        legacy_theme: Option<&str>,
    ) -> crate::Result<()> {
        match self.version {
            1 => {
                let quoter = shlex::Quoter::new().allow_nul(true);

                // Previously split by spaces
                if let Some(pre_launch) = self.hooks.pre_launch.as_ref() {
                    self.hooks.pre_launch =
                        Some(quoter.join(pre_launch.split(' ')).unwrap())
                }

                // Previously treated as complete path to command
                if let Some(wrapper) = self.hooks.wrapper.as_ref() {
                    self.hooks.wrapper =
                        Some(quoter.quote(wrapper).unwrap().to_string())
                }

                // Previously split by spaces
                if let Some(post_exit) = self.hooks.post_exit.as_ref() {
                    self.hooks.post_exit =
                        Some(quoter.join(post_exit.split(' ')).unwrap())
                }

                self.version = 2;
            }
            2 => {
                // Update old default memory setting from 2GB to 4GB (depending on system memory)
                const LEGACY_DEFAULT_MEMORY_MB: u32 = 2048;
                if self.memory.maximum == LEGACY_DEFAULT_MEMORY_MB {
                    self.memory.maximum =
                        crate::api::jre::default_memory_max_mb();
                }

                self.version = 3;
            }
            3 => {
                self.version = 4;
            }
            4 => {
                // Translate the old Dark/Light/Oled/System enum into the new
                // hex-based color_theme, reusing the exact surface-1 hex
                // values from variables.scss so upgrading users land on
                // visually identical surfaces rather than a jarring reset.
                self.color_theme = match legacy_theme {
                    Some("light") => "#ebebeb",
                    Some("oled") => "#000000",
                    // "dark", "system", or unknown all mapped to the dark palette
                    _ => "#16181c",
                }
                .to_string();
                self.dark_color_theme = "#16181c".to_string();
                self.dark_brand_color = self.brand_color.clone();

                self.version = 5;
            }
            version => {
                return Err(crate::ErrorKind::OtherError(format!(
                    "Invalid settings version: {version}"
                ))
                .into());
            }
        }

        Ok(())
    }
}

/// Minecraft memory settings
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct MemorySettings {
    pub maximum: u32,
}

/// Game window size
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WindowSize(pub u16, pub u16);

/// Game initialization hooks
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde_with::serde_as]
pub struct Hooks {
    #[serde_as(as = "serde_with::NoneAsEmptyString")]
    pub pre_launch: Option<String>,
    #[serde_as(as = "serde_with::NoneAsEmptyString")]
    pub wrapper: Option<String>,
    #[serde_as(as = "serde_with::NoneAsEmptyString")]
    pub post_exit: Option<String>,
}

/// Opening window to start with
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum DefaultPage {
    Home,
    Library,
}

impl DefaultPage {
    pub fn as_str(&self) -> &'static str {
        match self {
            DefaultPage::Home => "home",
            DefaultPage::Library => "library",
        }
    }

    pub fn from_string(string: &str) -> Self {
        match string {
            "home" => Self::Home,
            "library" => Self::Library,
            _ => Self::Home,
        }
    }
}

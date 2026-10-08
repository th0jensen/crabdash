//! Saved application preferences, shared by desktop launches and running windows.
use anyhow::{Context as _, Result, bail};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::Duration};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub start_minimised: bool,
    pub close_to_tray: bool,
    pub auto_refresh: bool,
    pub refresh_seconds: u64,
    pub system_refresh_seconds: u64,
    pub log_lines: u32,
    pub terminal_font: String,
    pub terminal_font_size: f32,
    pub terminal_line_height: f32,
    pub terminal_type: String,
    pub true_color: bool,
    pub scrollback_lines: u32,
    pub terminal_rows: u16,
    pub liquid_glass: bool,
    pub use_system_accent: bool,
    pub interface_font: String,
    pub interface_font_size: f32,
    pub sidebar_width: f32,
    pub tab_width: f32,
    pub always_show_shortcuts: bool,
}
impl Global for Preferences {}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            start_minimised: false,
            close_to_tray: true,
            auto_refresh: true,
            refresh_seconds: 5,
            system_refresh_seconds: 2,
            log_lines: 500,
            terminal_font: "JetBrainsMono Nerd Font".into(),
            terminal_font_size: 13.0,
            terminal_line_height: 1.5,
            terminal_type: machines::terminal::TERMINAL_TYPE.into(),
            true_color: true,
            scrollback_lines: 5000,
            terminal_rows: 16,
            liquid_glass: true,
            use_system_accent: true,
            interface_font: String::new(),
            interface_font_size: 14.0,
            sidebar_width: 240.0,
            tab_width: 144.0,
            always_show_shortcuts: false,
        }
    }
}

impl Preferences {
    pub fn path() -> Result<PathBuf> {
        super::platform::path()
    }

    pub fn load() -> Result<Self> {
        let contents = match fs::read(Self::path()?) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error).context("Unable to read preferences"),
        };
        let preferences: Self =
            serde_json::from_slice(&contents).context("Unable to read preferences")?;
        preferences.validate()?;
        Ok(preferences)
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let path = Self::path()?;
        fs::create_dir_all(path.parent().context("Missing preferences directory")?)
            .context("Unable to create preferences directory")?;
        let temporary = path.with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)
            .context("Unable to save preferences")?;
        fs::rename(temporary, path).context("Unable to save preferences")
    }

    pub fn validate(&self) -> Result<()> {
        fn range(label: &str, value: f64, min: f64, max: f64) -> Result<()> {
            if !value.is_finite() || !(min..=max).contains(&value) {
                bail!("{label} must be between {min} and {max}.");
            }
            Ok(())
        }
        range("Refresh interval", self.refresh_seconds as f64, 2.0, 300.0)?;
        range(
            "System sample interval",
            self.system_refresh_seconds as f64,
            2.0,
            60.0,
        )?;
        range("Log lines", self.log_lines as f64, 50.0, 10000.0)?;
        range(
            "Terminal font size",
            self.terminal_font_size.into(),
            8.0,
            32.0,
        )?;
        range(
            "Terminal line height",
            self.terminal_line_height.into(),
            1.0,
            2.5,
        )?;
        range(
            "Scrollback lines",
            self.scrollback_lines as f64,
            0.0,
            100000.0,
        )?;
        range("Terminal rows", self.terminal_rows.into(), 4.0, 48.0)?;
        range(
            "Interface font size",
            self.interface_font_size.into(),
            10.0,
            20.0,
        )?;
        range("Sidebar width", self.sidebar_width.into(), 180.0, 420.0)?;
        range("Tab width", self.tab_width.into(), 120.0, 240.0)?;
        for family in [&self.terminal_font, &self.interface_font] {
            if family.len() > 200 || family.chars().any(char::is_control) {
                bail!(
                    "Font names must be shorter than 200 characters and contain no control characters."
                );
            }
        }
        if self.terminal_font.trim().is_empty() {
            bail!("Choose a terminal font.");
        }
        machines::terminal::TerminalOptions {
            terminal_type: self.terminal_type.clone(),
            true_color: self.true_color,
        }
        .validate()?;
        Ok(())
    }

    pub(crate) fn system_refresh_interval(&self) -> Duration {
        Duration::from_secs(self.system_refresh_seconds)
    }

    pub(crate) fn terminal_options(&self) -> machines::terminal::TerminalOptions {
        machines::terminal::TerminalOptions {
            terminal_type: self.terminal_type.clone(),
            true_color: self.true_color,
        }
    }
}

pub(crate) fn current(cx: &App) -> Preferences {
    cx.try_global::<Preferences>().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_preferences_migrate_with_compatible_defaults() -> Result<()> {
        let settings: Preferences =
            serde_json::from_str(r#"{"start_minimised":true,"refresh_seconds":15}"#)?;
        assert!(settings.start_minimised);
        assert!(settings.liquid_glass);
        assert!(settings.use_system_accent);
        assert_eq!(settings.terminal_type, "xterm-256color");
        assert_eq!(settings.terminal_font, "JetBrainsMono Nerd Font");
        assert_eq!(settings.refresh_seconds, 15);
        assert_eq!(settings.system_refresh_seconds, 2);
        assert_eq!(settings.system_refresh_interval(), Duration::from_secs(2));
        settings.validate()?;
        assert_eq!(
            serde_json::from_slice::<Preferences>(&serde_json::to_vec(&settings)?)?,
            settings
        );
        Ok(())
    }
    #[test]
    fn macos_appearance_options_are_independent_and_survive_saving() -> Result<()> {
        for glass in [false, true] {
            for accent in [false, true] {
                let settings: Preferences = serde_json::from_str(&format!(
                    r#"{{"liquid_glass":{glass},"use_system_accent":{accent}}}"#
                ))?;
                let restored: Preferences =
                    serde_json::from_slice(&serde_json::to_vec(&settings)?)?;
                assert_eq!(restored.liquid_glass, glass);
                assert_eq!(restored.use_system_accent, accent);
            }
        }
        Ok(())
    }

    #[test]
    fn invalid_settings_do_not_reach_the_renderer_or_shell() {
        let mut settings = Preferences::default();
        settings.terminal_font_size = f32::NAN;
        assert!(settings.validate().is_err());
        settings = Preferences::default();
        settings.terminal_type = "xterm\ncommand".into();
        assert!(settings.validate().is_err());
        settings = Preferences::default();
        settings.refresh_seconds = 0;
        assert!(settings.validate().is_err());
    }
}

#[cfg(test)]
mod system_tests {
    use super::*;
    #[test]
    fn system_interval_validation_and_roundtrip_preserve_independent_refresh() -> Result<()> {
        for seconds in [0, 1, 61, u64::MAX] {
            let settings = Preferences {
                system_refresh_seconds: seconds,
                ..Preferences::default()
            };
            assert!(
                settings.validate().is_err(),
                "must reject interval {seconds}"
            );
        }
        for seconds in [2, 17, 60] {
            let settings = Preferences {
                system_refresh_seconds: seconds,
                refresh_seconds: 300,
                auto_refresh: false,
                ..Preferences::default()
            };
            settings.validate()?;
            let saved: Preferences = serde_json::from_slice(&serde_json::to_vec(&settings)?)?;
            assert_eq!(
                saved.system_refresh_interval(),
                Duration::from_secs(seconds)
            );
            assert_eq!(saved.refresh_seconds, 300);
            assert!(!saved.auto_refresh);
        }
        Ok(())
    }
}

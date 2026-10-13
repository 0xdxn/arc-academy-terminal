//! Configuration management for Arc Academy Terminal

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// General application settings
    #[serde(default)]
    pub general: GeneralConfig,

    /// Theme configuration
    #[serde(default)]
    pub theme: ThemeConfig,

    /// AI integration settings
    #[serde(default)]
    pub ai: AIConfig,

    /// Telemetry settings
    #[serde(default)]
    pub telemetry: TelemetryConfig,

    /// Shell settings
    #[serde(default)]
    pub shell: ShellConfig,

    /// Lesson practice settings
    #[serde(default)]
    pub lessons: LessonsConfig,

    /// Keybinding customization
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
}

/// General application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// User's name for personalization
    #[serde(default)]
    pub user_name: Option<String>,

    /// Whether first-run setup is complete
    #[serde(default = "default_false")]
    pub setup_complete: bool,

    /// Default shell to use (bash, zsh, fish, etc.)
    #[serde(default = "default_shell")]
    pub shell: String,

    /// Command history limit
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,

    /// Command timeout in seconds
    #[serde(default = "default_command_timeout")]
    pub command_timeout: u64,

    /// Enable auto-save for session
    #[serde(default = "default_true")]
    pub auto_save: bool,
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Default theme name
    #[serde(default = "default_theme")]
    pub default_theme: String,

    /// Enable ANSI colors
    #[serde(default = "default_true")]
    pub enable_colors: bool,

    /// Color depth (16, 256, or "true")
    #[serde(default = "default_color_depth")]
    pub color_depth: String,

    /// Use Nerd Font glyphs for icons (needs a patched font). Off by
    /// default: plain one-cell symbols work in every terminal.
    #[serde(default = "default_false")]
    pub nerd_fonts: bool,
}

/// AI integration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    /// Enable AI assistant
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// AI provider (anthropic, openai, local, managed)
    #[serde(default = "default_ai_provider")]
    pub provider: String,

    /// API key (use env var for security)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Model name
    #[serde(default)]
    pub model: Option<String>,

    /// Custom API endpoint (for local/self-hosted)
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Max tokens per request
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

/// Telemetry settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Enable telemetry (opt-in)
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// Anonymous user ID
    #[serde(default)]
    pub user_id: Option<String>,

    /// Send usage statistics
    #[serde(default = "default_false")]
    pub usage_stats: bool,

    /// Send error reports
    #[serde(default = "default_false")]
    pub error_reports: bool,
}

/// Shell settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Persistent environment variables
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Persistent aliases
    #[serde(default)]
    pub aliases: HashMap<String, String>,

    /// Startup commands to run
    #[serde(default)]
    pub startup_commands: Vec<String>,

    /// Safe mode (default on): the free shell runs inside ~/ArcAcademy/home
    /// and the safety guard keeps commands there. Off: commands run in your
    /// real directories, and only catastrophic patterns are refused.
    #[serde(default = "default_true")]
    pub safe_mode: bool,
}

/// Lesson practice settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonsConfig {
    /// Where lesson commands execute: "simulated" (safe virtual sandbox,
    /// the default) or "real" (real shell inside ~/ArcAcademy/playground)
    #[serde(default = "default_practice_mode")]
    pub practice_mode: String,

    /// Learning track shown first: "explorer" (younger learners, story
    /// lessons) or "operator" (teens and adults)
    #[serde(default = "default_track")]
    pub track: String,

    /// Whether the one-time real-mode explainer has been shown
    #[serde(default = "default_false")]
    pub real_mode_intro_shown: bool,
}

impl LessonsConfig {
    /// True when lesson practice runs on the real filesystem playground
    pub fn is_real(&self) -> bool {
        self.practice_mode == "real"
    }
}

/// Keybinding customization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    /// Custom keybindings (key -> action)
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

impl Config {
    /// Create a new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from disk
    pub fn load() -> Result<Self> {
        let config_path = get_config_file_path()?;

        if !config_path.exists() {
            // Create default config if it doesn't exist
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        Self::load_from(&config_path)
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        let config_str = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let mut config: Config = toml::from_str(&config_str)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        // Override with environment variables
        config.apply_env_overrides();

        Ok(config)
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_path = get_config_file_path()?;
        self.save_to(&config_path)
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &Path) -> Result<()> {
        // Ensure config directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let config_str = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(path, config_str)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        // The config may contain secrets (ai.api_key) — restrict it to
        // owner read/write on both create and rewrite
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .with_context(|| {
                    format!("Failed to set permissions on config file: {}", path.display())
                })?;
        }

        Ok(())
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // AI API key from environment
        if let Ok(api_key) = std::env::var("ARCT_AI_API_KEY") {
            self.ai.api_key = Some(api_key);
        }

        // AI provider from environment
        if let Ok(provider) = std::env::var("ARCT_AI_PROVIDER") {
            self.ai.provider = provider;
        }

        // Telemetry opt-out
        if let Ok(telemetry) = std::env::var("ARCT_TELEMETRY") {
            self.telemetry.enabled = telemetry == "1" || telemetry.to_lowercase() == "true";
        }

        // Shell override
        if let Ok(shell) = std::env::var("ARCT_SHELL") {
            self.general.shell = shell;
        }
    }

    /// Get config file path
    pub fn config_path() -> Result<PathBuf> {
        get_config_file_path()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            theme: ThemeConfig::default(),
            ai: AIConfig::default(),
            telemetry: TelemetryConfig::default(),
            shell: ShellConfig::default(),
            lessons: LessonsConfig::default(),
            keybindings: KeybindingsConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            user_name: None,
            setup_complete: false,
            shell: default_shell(),
            history_limit: default_history_limit(),
            command_timeout: default_command_timeout(),
            auto_save: true,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            default_theme: default_theme(),
            enable_colors: true,
            color_depth: default_color_depth(),
            nerd_fonts: false,
        }
    }
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_ai_provider(),
            api_key: None,
            model: None,
            endpoint: None,
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            user_id: None,
            usage_stats: false,
            error_reports: false,
        }
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            environment: HashMap::new(),
            aliases: HashMap::new(),
            startup_commands: Vec::new(),
            safe_mode: true,
        }
    }
}

impl Default for LessonsConfig {
    fn default() -> Self {
        Self {
            practice_mode: default_practice_mode(),
            track: default_track(),
            real_mode_intro_shown: false,
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            custom: HashMap::new(),
        }
    }
}

/// Get the configuration file path (XDG-compliant)
pub fn get_config_file_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?;

    let arct_config_dir = config_dir.join("arct");

    if !arct_config_dir.exists() {
        fs::create_dir_all(&arct_config_dir)
            .with_context(|| format!("Failed to create config directory: {}", arct_config_dir.display()))?;
    }

    Ok(arct_config_dir.join("config.toml"))
}

/// Generate a default configuration file as a string
pub fn generate_default_config() -> String {
    let config = Config::default();
    toml::to_string_pretty(&config).unwrap_or_else(|_| String::from("# Failed to generate config"))
}

// Default value functions
fn default_shell() -> String {
    std::env::var("SHELL")
        .unwrap_or_else(|_| "bash".to_string())
        .split('/')
        .last()
        .unwrap_or("bash")
        .to_string()
}

fn default_history_limit() -> usize {
    1000
}

fn default_command_timeout() -> u64 {
    5
}

fn default_theme() -> String {
    "Arc Academy Orange".to_string()
}

fn default_color_depth() -> String {
    "256".to_string()
}

fn default_ai_provider() -> String {
    "anthropic".to_string()
}

fn default_max_tokens() -> usize {
    4096
}

fn default_practice_mode() -> String {
    "simulated".to_string()
}

fn default_track() -> String {
    "explorer".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.general.history_limit, 1000);
        assert_eq!(config.theme.default_theme, "Arc Academy Orange");
        assert!(!config.ai.enabled);
        assert!(!config.telemetry.enabled);
    }

    #[test]
    fn test_lessons_practice_mode_defaults_to_simulated() {
        let config = Config::default();
        assert_eq!(config.lessons.practice_mode, "simulated");
        assert!(!config.lessons.is_real());
        assert!(!config.lessons.real_mode_intro_shown);

        // Configs written before the [lessons] section existed still parse
        let legacy = "[general]\nshell = \"zsh\"\n";
        let parsed: Config = toml::from_str(legacy).unwrap();
        assert_eq!(parsed.lessons.practice_mode, "simulated");

        // And the key is settable
        let toml_str = "[lessons]\npractice_mode = \"real\"\n";
        let parsed: Config = toml::from_str(toml_str).unwrap();
        assert!(parsed.lessons.is_real());
    }

    #[test]
    fn test_serialize_config() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("[general]"));
        assert!(toml_str.contains("[theme]"));
    }

    #[test]
    fn test_deserialize_config() {
        let toml_str = r#"
            [general]
            shell = "zsh"
            history_limit = 500

            [theme]
            default_theme = "Arc Dark"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.shell, "zsh");
        assert_eq!(config.general.history_limit, 500);
        assert_eq!(config.theme.default_theme, "Arc Dark");
    }

    /// A config path in a fresh temp directory, removed when the test ends
    struct TempConfig {
        dir: PathBuf,
        path: PathBuf,
    }

    impl std::ops::Deref for TempConfig {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.path
        }
    }

    impl AsRef<std::path::Path> for TempConfig {
        fn as_ref(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn temp_config_path(name: &str) -> TempConfig {
        let dir = std::env::temp_dir().join(format!(
            "arct-config-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("config.toml");
        TempConfig { dir, path }
    }

    #[test]
    fn test_save_to_and_load_from_roundtrip() {
        let path = temp_config_path("roundtrip");

        let mut config = Config::default();
        config.general.user_name = Some("tester".to_string());
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.general.user_name.as_deref(), Some("tester"));
    }

    #[test]
    fn test_load_from_missing_path_errors() {
        let path = temp_config_path("missing");
        assert!(Config::load_from(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_save_sets_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_config_path("perms");

        let config = Config::default();
        config.save_to(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        // Rewriting an existing file keeps the restrictive permissions
        config.save_to(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

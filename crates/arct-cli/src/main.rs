//! Arc Academy Terminal - CLI Entry Point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ABOUT: &str = "🎓 Arc Academy Terminal - Learn shell commands interactively";

#[derive(Parser)]
#[command(name = "arct")]
#[command(version = VERSION)]
#[command(about = ABOUT, long_about = None)]
#[command(author = "Arc Academy")]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Config file path (defaults to ~/.config/arct/config.toml)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the interactive TUI (default)
    #[command(visible_alias = "tui")]
    Start {
        /// Start with a specific theme
        #[arg(long)]
        theme: Option<String>,

        /// Working directory to start in
        #[arg(long)]
        dir: Option<PathBuf>,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Quick command explanation (non-interactive)
    Explain {
        /// Command to explain
        command: String,
    },

    /// Show version information
    Version,

    /// Show configuration paths and system info
    Info,

    /// Manage telemetry data
    Telemetry {
        #[command(subcommand)]
        action: TelemetryAction,
    },

    /// First Keys: a safe screen for children who can't read yet.
    /// Letters appear big and are spoken; a grown-up leaves with Ctrl+G, then q u i t
    FirstKeys,

    /// Make a printable certificate for a finished track, or for First Keys
    Certificate {
        /// Track to certify, or first-keys (default: every finished track and First Keys)
        #[arg(long, value_enum)]
        track: Option<TrackArg>,

        /// Name to print (default: the name from setup, except on First Keys; none leaves a line to write on)
        #[arg(long)]
        name: Option<String>,

        /// Where to write it: a folder or an .html file (default: ~/ArcAcademy/certificates)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Print a plain-text certificate instead of writing a page
        #[arg(long)]
        text: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum TrackArg {
    Explorer,
    Operator,
    FirstKeys,
}

impl From<TrackArg> for arct_tui::certificate::Award {
    fn from(t: TrackArg) -> Self {
        use arct_tui::certificate::Award;
        match t {
            TrackArg::Explorer => Award::Track(arct_core::Track::Explorer),
            TrackArg::Operator => Award::Track(arct_core::Track::Operator),
            TrackArg::FirstKeys => Award::FirstKeys,
        }
    }
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,

    /// Edit configuration file
    Edit,

    /// Reset configuration to defaults
    Reset {
        /// Skip confirmation
        #[arg(short, long)]
        yes: bool,
    },

    /// Get configuration file path
    Path,

    /// Generate default configuration
    Init {
        /// Overwrite existing config
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum TelemetryAction {
    /// Show telemetry statistics
    Stats,

    /// Export all telemetry data as JSON
    Export {
        /// Output file (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Delete all telemetry data
    Delete {
        /// Skip confirmation
        #[arg(short, long)]
        yes: bool,
    },

    /// Show telemetry database path
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    if let Some(ref config_path) = cli.config {
        tracing::info!("Using config from: {}", config_path.display());
    }

    match cli.command {
        None | Some(Commands::Start { .. }) | Some(Commands::FirstKeys) => {
            // Default: run TUI
            run_tui(cli.command, cli.config).await?;
        }
        Some(Commands::Config { action }) => {
            handle_config(action, cli.config.as_deref())?;
        }
        Some(Commands::Explain { command }) => {
            handle_explain(&command)?;
        }
        Some(Commands::Version) => {
            print_version();
        }
        Some(Commands::Info) => {
            print_info()?;
        }
        Some(Commands::Telemetry { action }) => {
            handle_telemetry(action)?;
        }
        Some(Commands::Certificate { track, name, out, text }) => {
            let made = handle_certificate(track, name, out, text, cli.config.as_deref())?;
            if !made {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Make certificates for finished tracks. Returns false when none could be made.
fn handle_certificate(
    track: Option<TrackArg>,
    name: Option<String>,
    out: Option<PathBuf>,
    text: bool,
    config_path: Option<&std::path::Path>,
) -> Result<bool> {
    use arct_core::Track;
    use arct_tui::certificate::{Award, Certificate};

    let session = arct_tui::persistence::load_session()?;
    let mut library = arct_core::LessonLibrary::new();
    if let Some(config_dir) = dirs::config_dir() {
        // Same lesson packs the app loads, so totals match what the learner saw
        let _ = library.load_from_dir(&config_dir.join("arct").join("lessons"));
    }
    let given_name = name.clone();
    let name = name.or_else(|| {
        let config = match config_path {
            Some(path) => arct_config::Config::load_from(path),
            None => arct_config::Config::load(),
        };
        config.ok().and_then(|c| c.general.user_name)
    });

    let awards: Vec<Award> = match track {
        Some(t) => vec![t.into()],
        None => vec![
            Award::Track(Track::Explorer),
            Award::Track(Track::Operator),
            Award::FirstKeys,
        ],
    };
    let mut certificates = Vec::new();
    let mut not_yet = Vec::new();
    for award in awards {
        let made = match award {
            Award::Track(t) => Certificate::for_track(&session.user_stats, &library, t, name.clone()),
            // The child in First Keys is rarely whoever set arct up: only a
            // name given here goes on it
            Award::FirstKeys => Certificate::first_keys(&session.user_stats, given_name.clone()),
        };
        match made {
            Ok(cert) => certificates.push(cert),
            // Unasked, an unearned First Keys certificate isn't worth a mention
            Err(why) if why.award == Award::FirstKeys && track.is_none() => {}
            Err(why) => not_yet.push(why),
        }
    }

    // Say what's left when asked for one track, or when nothing is finished
    if track.is_some() || certificates.is_empty() {
        for why in &not_yet {
            eprint!("{why}");
        }
    }
    if certificates.is_empty() {
        return Ok(false);
    }

    for cert in &certificates {
        if text {
            print!("{}", cert.to_text());
            continue;
        }
        let path = match &out {
            Some(p) if p.extension().is_some_and(|e| e == "html") => p.clone(),
            Some(dir) => dir.join(cert.file_name()),
            None => dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not find your home folder; use --out"))?
                .join("ArcAcademy")
                .join("certificates")
                .join(cert.file_name()),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, cert.to_html())?;
        println!("Certificate saved: {}", path.display());
    }
    if !text {
        println!("Open it in a web browser and print it. It was made on this computer; nothing was sent anywhere.");
    }
    Ok(true)
}

async fn run_tui(command: Option<Commands>, config_path: Option<PathBuf>) -> Result<()> {
    let mut options = arct_tui::RunOptions {
        config_path,
        theme: None,
        first_keys: matches!(command, Some(Commands::FirstKeys)),
    };

    // Apply any start options
    if let Some(Commands::Start { theme, dir }) = command {
        if let Some(theme_name) = theme {
            tracing::info!("Starting with theme: {}", theme_name);
            options.theme = Some(theme_name);
        }
        if let Some(working_dir) = dir {
            std::env::set_current_dir(&working_dir)?;
            tracing::info!("Starting in directory: {}", working_dir.display());
        }
    }

    // Run the TUI
    arct_tui::run_with_options(options).await?;

    Ok(())
}

fn handle_config(action: ConfigAction, config_override: Option<&std::path::Path>) -> Result<()> {
    // Honor a global --config <path> override; fall back to the default path
    let config_path = match config_override {
        Some(path) => path.to_path_buf(),
        None => arct_config::get_config_file_path()?,
    };

    match action {
        ConfigAction::Show => {
            let config = match config_override {
                Some(path) => arct_config::Config::load_from(path)?,
                None => arct_config::Config::load()?,
            };
            let toml_str = toml::to_string_pretty(&config)?;
            println!("{}", toml_str);
        }
        ConfigAction::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

            println!("Opening config in {}...", editor);
            std::process::Command::new(editor)
                .arg(&config_path)
                .status()?;
        }
        ConfigAction::Reset { yes } => {
            if !yes {
                print!("Reset configuration to defaults? [y/N] ");
                use std::io::{self, Write};
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let config = arct_config::Config::default();
            config.save_to(&config_path)?;
            println!("✓ Configuration reset to defaults");
        }
        ConfigAction::Path => {
            println!("{}", config_path.display());
        }
        ConfigAction::Init { force } => {
            if config_path.exists() && !force {
                eprintln!("Config file already exists: {}", config_path.display());
                eprintln!("Use --force to overwrite");
                return Ok(());
            }

            let config = arct_config::Config::default();
            config.save_to(&config_path)?;
            println!("✓ Created config file: {}", config_path.display());
        }
    }
    Ok(())
}

fn handle_explain(command_str: &str) -> Result<()> {
    use arct_core::{CommandAnalyzer, Educator};

    let analyzer = CommandAnalyzer::new();
    let mut educator = Educator::new();

    let cmd = analyzer.parse(command_str)?;
    let explanation = educator.explain(&cmd)?;

    println!("\n📚 Command: {}\n", cmd.program);
    println!("{}\n", explanation.summary);

    if !explanation.flag_explanations.is_empty() {
        println!("🔍 Flags:");
        for flag_exp in &explanation.flag_explanations {
            println!("  {} - {}", flag_exp.flag, flag_exp.description);
        }
        println!();
    }

    if !explanation.warnings.is_empty() {
        println!("⚠️  Warnings:");
        for warning in &explanation.warnings {
            println!("  • {} - {}", warning.severity, warning.message);
            if let Some(suggestion) = &warning.suggestion {
                println!("    → {}", suggestion);
            }
        }
        println!();
    }

    if !explanation.tips.is_empty() {
        println!("💡 Tips:");
        for tip in &explanation.tips {
            println!("  • {} - {}", tip.title, tip.content);
        }
        println!();
    }

    Ok(())
}

fn print_version() {
    println!("Arc Academy Terminal v{}", VERSION);
    println!();
    println!("🌐 arcacademy.sh");
    println!("📚 Learn shell commands interactively");
}

fn print_info() -> Result<()> {
    println!("Arc Academy Terminal - System Info");
    println!("==================================\n");

    println!("Version:  {}", VERSION);
    println!();

    // Config paths
    if let Ok(config_path) = arct_config::get_config_file_path() {
        println!("Config:   {}", config_path.display());
        println!("          {}", if config_path.exists() { "exists" } else { "not found" });
    }

    // Session paths
    if let Ok(session_path) = arct_tui::persistence::get_session_file_path() {
        println!("Session:  {}", session_path.display());
        println!("          {}", if session_path.exists() { "exists" } else { "not found" });
    }

    println!();

    // System info
    println!("OS:       {}", std::env::consts::OS);
    println!("Arch:     {}", std::env::consts::ARCH);

    if let Ok(shell) = std::env::var("SHELL") {
        println!("Shell:    {}", shell);
    }

    if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        println!("User:     {}", user);
    }

    if let Ok(home) = std::env::var("HOME") {
        println!("Home:     {}", home);
    }

    // Your own lesson packs (*.toml) are loaded from here
    if let Some(config_dir) = dirs::config_dir() {
        let lessons = config_dir.join("arct").join("lessons");
        println!("Lessons:  {}", lessons.display());
        println!("          {}", if lessons.exists() { "exists" } else { "not created yet (make it to add your own lessons)" });
    }

    println!();
    println!("Website:  https://arcacademy.sh");
    println!("Docs:     https://arcacademy.sh/docs");
    println!("Source:   https://github.com/0xdxn/arc-academy-terminal");

    Ok(())
}

fn handle_telemetry(action: TelemetryAction) -> Result<()> {
    use arct_telemetry::Telemetry;

    match action {
        TelemetryAction::Stats => {
            // Load config to check if telemetry is enabled
            let config = arct_config::Config::load()?;

            if !config.telemetry.enabled {
                println!("⚠️  Telemetry is disabled");
                println!("Enable it in your config to collect usage statistics:");
                println!("  arct config edit");
                println!("  Set: telemetry.enabled = true");
                return Ok(());
            }

            let telemetry = Telemetry::new(true)?;
            let stats = telemetry.get_stats()?;

            println!("📊 Telemetry Statistics\n");
            println!("Sessions:  {}", stats.total_sessions);
            println!("Commands:  {}", stats.total_commands);
            println!("Errors:    {}", stats.total_errors);
            println!();

            if !stats.top_commands.is_empty() {
                println!("Top Commands:");
                for (cmd, count) in stats.top_commands.iter().take(10) {
                    println!("  {:20} {}", cmd, count);
                }
                println!();
            }

            if !stats.features_used.is_empty() {
                println!("Features Used:");
                for (feature, count) in stats.features_used.iter().take(10) {
                    println!("  {:20} {}", feature, count);
                }
                println!();
            }
        }
        TelemetryAction::Export { output } => {
            let telemetry = Telemetry::new(true)?;
            let data = telemetry.export_data()?;

            if let Some(output_path) = output {
                std::fs::write(&output_path, data)?;
                println!("✓ Telemetry data exported to: {}", output_path.display());
            } else {
                println!("{}", data);
            }
        }
        TelemetryAction::Delete { yes } => {
            if !yes {
                print!("Delete all telemetry data? This cannot be undone. [y/N] ");
                use std::io::{self, Write};
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let telemetry = Telemetry::new(true)?;
            telemetry.delete_all_data()?;
            println!("✓ All telemetry data deleted");
        }
        TelemetryAction::Path => {
            let path = arct_telemetry::get_telemetry_db_path()?;
            println!("{}", path.display());
        }
    }

    Ok(())
}

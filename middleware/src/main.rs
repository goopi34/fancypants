use clap::Parser;
use fancypants::{generate_config, load_config, log_config, reconnect_loop, RealSession};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "fancypants",
    version = env!("FANCYPANTS_VERSION"),
    about = "BLE rangefinder to Buttplug.io middleware",
    long_about = "Connects to a fancypants-nrf52 BLE rangefinder and translates distance \
                   readings into haptic intensity for toys via Intiface Engine or standalone BLE."
)]
struct Args {
    /// Path to TOML configuration file
    /// [default: ./config.toml, then $XDG_CONFIG_HOME/fancypants/config.toml]
    #[arg(short, long, verbatim_doc_comment)]
    config: Option<PathBuf>,

    /// Generate a default config file and exit
    /// (written to -c PATH, or ~/.config/fancypants/config.toml)
    #[arg(long, verbatim_doc_comment)]
    generate_config: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&args.log_level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Generate default config if requested
    if args.generate_config {
        let path = generate_config(args.config.as_deref())?;
        info!("Default config written to {:?}", path);
        return Ok(());
    }

    // Load config
    let config = load_config(args.config.as_deref())?;
    log_config(&config);

    // Ctrl+C handling
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        info!("Shutdown requested...");
        running_clone.store(false, Ordering::SeqCst);
    })?;

    // Main loop with reconnection
    reconnect_loop(&config, &running, RealSession).await;

    info!("Goodbye");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_defaults() {
        let args = Args::try_parse_from(["fancypants"]).unwrap();
        assert_eq!(args.config, None);
        assert!(!args.generate_config);
        assert_eq!(args.log_level, "info");
    }

    #[test]
    fn test_args_custom_config() {
        let args = Args::try_parse_from(["fancypants", "-c", "custom.toml"]).unwrap();
        assert_eq!(args.config, Some(PathBuf::from("custom.toml")));
    }

    #[test]
    fn test_args_generate_config() {
        let args = Args::try_parse_from(["fancypants", "--generate-config"]).unwrap();
        assert!(args.generate_config);
    }

    #[test]
    fn test_args_log_level() {
        let args = Args::try_parse_from(["fancypants", "-l", "debug"]).unwrap();
        assert_eq!(args.log_level, "debug");
    }
}

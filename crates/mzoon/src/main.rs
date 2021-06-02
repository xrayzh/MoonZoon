use std::env;
use structopt::StructOpt;
use anyhow::{Context, Result};
use tokio::{signal, time::Duration, join};
use std::process::Child;

mod config;
mod frontend;
mod backend;
mod file_compressor;
mod visit_files;
mod project_watcher;

use config::*;
use frontend::*;
use backend::*;

#[derive(Debug, StructOpt)]
enum Opt {
    New {
        project_name: String,
    },
    Start {
        #[structopt(short, long)]
        release: bool,
        #[structopt(short, long)]
        open: bool
    },
    Build {
        #[structopt(short, long)]
        release: bool,
    },
}

// Run from example:  cargo run --manifest-path "../../crates/mzoon/Cargo.toml" start

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::from_args();
    println!("{:?}", opt);

    match opt {
        Opt::New { .. } => {}
        Opt::Start { release, open } => {
            let config = Config::load_from_moonzoon_toml().await?;
            set_env_vars(&config, release);

            let debounce_time = Duration::from_millis(100);

            if let Err(error) = build_frontend(release, config.cache_busting).await {
                eprintln!("{}", error);
            }
            let (frontend_watcher, frontend_watcher_handle) = start_frontend_watcher(&config, release, debounce_time).await?;
            
            let mut server = None::<Child>;
            if let Err(error) = build_backend(release, config.https).await {
                eprintln!("{}", error);
            } else {
                match run_backend(release) {
                    Ok(backend) => {
                        if open {
                            let url = format!(
                                "{protocol}://localhost:{port}", 
                                protocol = if config.https { "https" } else { "http" },
                                port = config.port
                            );
                            println!("Open {} in the default web browser", url);
                            open::that(url).context("Failed to open the URL in the browser")?;
                        }
                        server = Some(backend)
                    }
                    Err(error) => {
                        eprintln!("{}", error);
                    }
                }
            }
            let (backend_watcher, backend_watcher_handle) = start_backend_watcher(&config, release, debounce_time, server).await?;

            signal::ctrl_c().await?;
            println!("Stopping mzoon...");
            let _ = join!(
                frontend_watcher.stop(),
                backend_watcher.stop(),
                frontend_watcher_handle,
                backend_watcher_handle,
            );
            println!("mzoon stopped");
        }
        Opt::Build { release } => {
            let config = Config::load_from_moonzoon_toml().await?;
            set_env_vars(&config, release);

            build_frontend(release, config.cache_busting).await?;
            build_backend(release, config.https).await?;
        }
    }
    Ok(())
}

fn set_env_vars(config: &Config, release: bool) {
    // port = 8443
    env::set_var("PORT", config.port.to_string());
    // https = true
    env::set_var("HTTPS", config.https.to_string());
    // backend_log_level = "warn"
    env::set_var("BACKEND_LOG_LEVEL", config.backend_log_level.as_str());

    // [redirect]
    // port = 8080
    env::set_var("REDIRECT_PORT", config.redirect.port.to_string());
    // enabled = true
    env::set_var("REDIRECT_ENABLED", config.redirect.enabled.to_string());

    env::set_var("COMPRESSED_PKG", release.to_string());
}

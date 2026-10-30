use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rivet_core::{
    CancellationToken, DownloadRequest, Engine, Progress, native,
    queue::{Queue, now},
    worker::{run_job, safe_error},
};
use std::{path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "Rivet — open source downloads, without activation or telemetry"
)]
struct Args {
    #[arg(long, global = true)]
    database: Option<PathBuf>,
    #[arg(long, global = true)]
    proxy: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(hide = true)]
    ApplyUpdate { plan: PathBuf },
    /// Download immediately; Ctrl-C preserves safely resumable parts.
    Get {
        url: String,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long, default_value_t = 4)]
        connections: u8,
        #[arg(long)]
        sha256: Option<String>,
    },
    /// Add a durable queue job. Adding does not initiate network activity.
    Add {
        url: String,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long, default_value_t = 4)]
        connections: u8,
        #[arg(long)]
        sha256: Option<String>,
        #[arg(long, default_value_t = 0)]
        delay_seconds: u64,
    },
    /// Show queue entries as JSON. May contain signed URLs: treat output as private.
    List,
    /// Process queued jobs; --once stops after the first available job.
    Run {
        #[arg(long)]
        once: bool,
    },
    /// Requeue a failed/paused job, or recover an expired worker lease.
    Retry { id: String },
    /// Remove a queue entry; downloaded and partial files are retained.
    Remove { id: String },
    /// Native messaging host entrypoint. stdout is reserved for browser framing.
    NativeHost {
        /// Browser-provided origin or extension ID (browser manifest enforces allowlist).
        _origin: Option<String>,
    },
    /// Print a native host manifest for explicit browser registration.
    HostManifest {
        #[arg(long)]
        executable: PathBuf,
        #[arg(long)]
        extension_id: String,
        #[arg(long)]
        firefox: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", safe_error(&error));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    if let Command::ApplyUpdate { plan } = &args.command {
        return rivet_core::update::apply(plan);
    }
    if let Command::HostManifest {
        executable,
        extension_id,
        firefox,
    } = &args.command
    {
        let path = executable
            .canonicalize()
            .context("executable path must exist")?;
        // Browser native manifests cannot supply command arguments. A dedicated host binary is provided.
        let mut manifest = serde_json::json!({"name":"org.rivet.downloads", "description":"Rivet download queue", "path":path, "type":"stdio"});
        if *firefox {
            manifest["allowed_extensions"] = serde_json::json!([extension_id]);
        } else {
            manifest["allowed_origins"] =
                serde_json::json!([format!("chrome-extension://{extension_id}/")]);
        }
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }
    let database = match args.database {
        Some(path) => path,
        None => Queue::default_path()?,
    };
    if let Command::NativeHost { .. } = args.command {
        return serve_host(database);
    }
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    match args.command {
        Command::Get {
            url,
            output,
            connections,
            sha256,
        } => {
            let request = DownloadRequest {
                url,
                destination: absolute(output)?,
                connections,
                expected_sha256: sha256,
            };
            let progress = Progress::default();
            let done = Engine::new(args.proxy.as_deref())?
                .download(request, cancel, progress)
                .await?;
            println!("{}", serde_json::to_string_pretty(&done)?);
        }
        Command::Add {
            url,
            output,
            connections,
            sha256,
            delay_seconds,
        } => {
            let request = DownloadRequest {
                url,
                destination: absolute(output)?,
                connections,
                expected_sha256: sha256,
            };
            println!(
                "{}",
                Queue::open(database)?.enqueue(
                    request,
                    now().saturating_add(
                        i64::try_from(delay_seconds).context("delay is too large")?
                    )
                )?
            );
        }
        Command::List => println!(
            "{}",
            serde_json::to_string_pretty(&Queue::open(database)?.list()?)?
        ),
        Command::Retry { id } => Queue::open(database)?.retry(&id)?,
        Command::Remove { id } => Queue::open(database)?.remove(&id)?,
        Command::Run { once } => {
            let queue = Queue::open(database)?;
            let engine = Engine::new(args.proxy.as_deref())?;
            while !cancel.is_cancelled() {
                if let Some(id) = queue.next_queued()? {
                    let result = run_job(
                        queue.clone(),
                        &id,
                        engine.clone(),
                        cancel.child_token(),
                        Progress::default(),
                    )
                    .await;
                    match result {
                        Ok(done) => println!("{}", serde_json::to_string(&done)?),
                        Err(e) if once => return Err(e),
                        Err(e) => eprintln!("{id}: {}", safe_error(&e)),
                    }
                    if once {
                        break;
                    }
                } else if once {
                    break;
                }
                tokio::select! { _ = cancel.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
            }
        }
        Command::NativeHost { .. } | Command::HostManifest { .. } | Command::ApplyUpdate { .. } => {
            unreachable!()
        }
    }
    Ok(())
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn serve_host(database: PathBuf) -> Result<()> {
    let downloads = native::default_download_dir()?;
    native::serve_with_actions(
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &Queue::open(&database)?,
        &downloads,
        || {},
        || rivet_core::desktop::ensure_running(&database),
    )
}

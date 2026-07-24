use std::{collections::BTreeMap, env, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local, TimeZone, Utc};
use clap::{Args, Parser, Subcommand};
use tracker::{
    db::Database,
    default_database_path,
    model::TimeEntry,
    sync::{serve, sync_with_peer},
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Override the local SQLite database path.
    #[arg(long, global = true, env = "TRACKER_DATABASE")]
    database: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create or update a reusable task.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Manage private sync peers.
    Peer {
        #[command(subcommand)]
        command: PeerCommand,
    },
    /// Start tracking a task.
    Start(TrackingArgs),
    /// Stop the running timer.
    Stop {
        /// Stop locally without attempting automatic peer sync.
        #[arg(long)]
        no_sync: bool,
    },
    /// Show the running timer.
    Status,
    /// List time entries and totals.
    Report {
        /// Beginning of the report: today, Nd (for example 7d), or RFC 3339.
        #[arg(long, default_value = "today")]
        since: String,
    },
    /// Run the authenticated sync API.
    Serve {
        /// Address to listen on. Use this device's Tailscale IP to serve the tailnet.
        #[arg(long, default_value = "127.0.0.1:7789")]
        bind: SocketAddr,
    },
    /// Exchange local records with a peer.
    Sync {
        /// Ad-hoc peer base URL. Omit to sync all saved peers.
        #[arg(long)]
        peer: Option<String>,
    },
    /// Print this installation's local paths and identifier.
    Info,
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Add a task, or update its defaults when the name already exists.
    Add(TrackingArgs),
    /// List reusable tasks.
    List,
}

#[derive(Subcommand)]
enum PeerCommand {
    /// Add a peer or update its URL.
    Add {
        /// A local nickname for this peer.
        name: String,
        /// Peer base URL, usually the private HTTPS URL from Tailscale Serve.
        url: String,
    },
    /// List saved peers.
    List,
    /// Remove a saved peer.
    Remove {
        /// Local peer nickname.
        name: String,
    },
}

#[derive(Args)]
struct TrackingArgs {
    /// Task name.
    name: String,
    /// Optional overarching project.
    #[arg(short, long)]
    project: Option<String>,
    /// Repeat to attach multiple tags.
    #[arg(short, long)]
    tag: Vec<String>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let database_path = cli.database.map(Ok).unwrap_or_else(default_database_path)?;
    let mut database = Database::open(&database_path)?;

    match cli.command {
        Command::Task {
            command: TaskCommand::Add(args),
        } => {
            let task = database.add_task(&args.name, args.project.as_deref(), &args.tag)?;
            println!("Saved task “{}”.", task.name);
        }
        Command::Task {
            command: TaskCommand::List,
        } => {
            let tasks = database.list_tasks()?;
            if tasks.is_empty() {
                println!("No tasks yet.");
            }
            for task in tasks {
                println!(
                    "{}{}{}",
                    task.name,
                    task.project
                        .map(|project| format!("  [{project}]"))
                        .unwrap_or_default(),
                    display_tags(&task.tags)
                );
            }
        }
        Command::Peer {
            command: PeerCommand::Add { name, url },
        } => {
            let peer = database.add_peer(&name, &url)?;
            println!("Saved peer “{}” at {}.", peer.name, peer.url);
        }
        Command::Peer {
            command: PeerCommand::List,
        } => {
            let peers = database.list_peers()?;
            if peers.is_empty() {
                println!("No sync peers configured.");
            }
            for peer in peers {
                println!("{}  {}", peer.name, peer.url);
            }
        }
        Command::Peer {
            command: PeerCommand::Remove { name },
        } => {
            if database.remove_peer(&name)? {
                println!("Removed peer “{name}”.");
            } else {
                bail!("no peer named “{name}”");
            }
        }
        Command::Start(args) => {
            let entry = database.start(&args.name, args.project.as_deref(), &args.tag)?;
            println!(
                "Started “{}” at {}.{}{}",
                entry.task_name,
                display_time(entry.started_at),
                display_project(entry.project.as_deref()),
                display_tags(&entry.tags)
            );
        }
        Command::Stop { no_sync } => {
            let entries = database.stop()?;
            if entries.len() == 1 {
                let entry = &entries[0];
                println!(
                    "Stopped “{}” after {}.",
                    entry.task_name,
                    display_duration(entry.elapsed_seconds_at(Utc::now()))
                );
            } else {
                println!(
                    "Stopped {} concurrently active timers from an offline sync conflict.",
                    entries.len()
                );
            }
            if !no_sync {
                sync_saved_peers(&mut database).await?;
            }
        }
        Command::Status => match database.active_entry()? {
            Some(entry) => println!(
                "Tracking “{}” for {}.{}{}",
                entry.task_name,
                display_duration(entry.elapsed_seconds_at(Utc::now())),
                display_project(entry.project.as_deref()),
                display_tags(&entry.tags)
            ),
            None => println!("No timer is running."),
        },
        Command::Report { since } => {
            print_report(&database.entries_since(parse_since(&since)?)?);
        }
        Command::Serve { bind } => {
            let token = sync_token()?;
            println!("Serving authenticated sync on http://{bind}");
            println!("Press Ctrl-C to stop.");
            drop(database);
            serve(database_path, bind, token).await?;
        }
        Command::Sync { peer: Some(peer) } => {
            sync_one(&mut database, "ad-hoc", &peer, &sync_token()?).await?;
        }
        Command::Sync { peer: None } => {
            let count = sync_saved_peers(&mut database).await?;
            if count == 0 {
                bail!("no saved peers; add one with `tracker peer add NAME URL`");
            }
        }
        Command::Info => {
            println!("Database: {}", database.path().display());
            println!("Device ID: {}", database.device_id()?);
            println!(
                "Sync token: {}",
                if env::var_os("TRACKER_SYNC_TOKEN").is_some() {
                    "set in environment"
                } else {
                    "not set"
                }
            );
        }
    }
    Ok(())
}

fn sync_token() -> Result<String> {
    env::var("TRACKER_SYNC_TOKEN").context(
        "TRACKER_SYNC_TOKEN is not set; generate one with `openssl rand -hex 32` and use the same value on each trusted device",
    )
}

async fn sync_saved_peers(database: &mut Database) -> Result<usize> {
    let peers = database.list_peers()?;
    if peers.is_empty() {
        return Ok(0);
    }
    let token = sync_token()?;
    for peer in &peers {
        sync_one(database, &peer.name, &peer.url, &token).await?;
    }
    Ok(peers.len())
}

async fn sync_one(database: &mut Database, name: &str, url: &str, token: &str) -> Result<()> {
    let summary = sync_with_peer(database, url, token)
        .await
        .with_context(|| format!("sync with peer “{name}” failed"))?;
    println!(
        "Synced “{}”: {} task change(s), {} time-entry change(s) received.",
        name, summary.tasks_applied, summary.entries_applied
    );
    Ok(())
}

fn parse_since(value: &str) -> Result<DateTime<Utc>> {
    if value.eq_ignore_ascii_case("today") {
        let local_midnight = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .context("could not calculate local midnight")?;
        return Local
            .from_local_datetime(&local_midnight)
            .earliest()
            .context("could not resolve local midnight")
            .map(|time| time.with_timezone(&Utc));
    }
    if let Some(days) = value.strip_suffix('d') {
        let days: u64 = days
            .parse()
            .context("day count must be a positive number")?;
        let duration = Duration::from_secs(days.saturating_mul(24 * 60 * 60));
        return Ok(
            Utc::now() - chrono::Duration::from_std(duration).context("day count is too large")?
        );
    }
    value
        .parse::<DateTime<Utc>>()
        .context("use today, Nd (such as 7d), or an RFC 3339 timestamp")
}

fn print_report(entries: &[TimeEntry]) {
    if entries.is_empty() {
        println!("No entries in this period.");
        return;
    }
    let now = Utc::now();
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    for entry in entries {
        let elapsed = entry.elapsed_seconds_at(now);
        let project = entry.project.as_deref().unwrap_or("(no project)");
        *totals.entry(project.to_owned()).or_default() += elapsed;
        println!(
            "{}  {:>8}  {}{}",
            display_time(entry.started_at),
            display_duration(elapsed),
            entry.task_name,
            if entry.stopped_at.is_none() {
                " (running)"
            } else {
                ""
            }
        );
    }
    println!();
    println!("Totals by project:");
    let mut grand_total = 0;
    for (project, seconds) in totals {
        grand_total += seconds;
        println!("  {:<24} {}", project, display_duration(seconds));
    }
    println!("  {:<24} {}", "Total", display_duration(grand_total));
}

fn display_time(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn display_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn display_project(project: Option<&str>) -> String {
    project
        .map(|project| format!(" Project: {project}."))
        .unwrap_or_default()
}

fn display_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        String::new()
    } else {
        format!(" #{}", tags.join(" #"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_days() {
        let parsed = parse_since("7d").unwrap();
        let difference = Utc::now().signed_duration_since(parsed).num_days();
        assert!((6..=7).contains(&difference));
    }

    #[test]
    fn formats_duration() {
        assert_eq!(display_duration(3_661), "01:01:01");
    }
}

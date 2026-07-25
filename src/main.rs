use std::{
    collections::BTreeMap, env, ffi::OsString, net::SocketAddr, path::PathBuf, time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local, TimeZone, Utc};
use clap::{Args, Parser, Subcommand};
use tracker::{
    db::Database,
    default_database_path,
    model::{TimeEntry, TrackerSnapshot},
    sync::{ReachablePeer, discover_tracker_peers, serve, sync_with_peer},
    tailscale::TailscaleStatus,
};

const DEFAULT_SYNC_PORT: u16 = 7789;

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
    /// Start tracking a task.
    ///
    /// Any unknown long flag is treated as a tag shortcut, so `--chess` is
    /// equivalent to `--tag chess`.
    Start(TrackingArgs),
    /// Stop the running timer.
    Stop,
    /// Show the running timer.
    Status,
    /// List time entries and totals.
    Report {
        /// Beginning of the report: today, Nd (for example 7d), or RFC 3339.
        #[arg(long, default_value = "today")]
        since: String,
    },
    /// Print a machine-readable snapshot for desktop app integrations.
    Snapshot {
        /// Beginning of the snapshot: today, Nd, or RFC 3339.
        #[arg(long, default_value = "7d")]
        since: String,
    },
    /// Make this database available to other Tracker devices on the tailnet.
    Serve {
        /// Advanced: override automatic binding to this device's Tailscale IP.
        #[arg(long)]
        bind: Option<SocketAddr>,
    },
    /// Exchange local records with a peer.
    Sync {
        /// Advanced: sync only this URL instead of discovering tailnet devices.
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
    let cli = Cli::parse_from(normalize_start_tag_shortcuts(env::args_os()));
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
        Command::Stop => {
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
        Command::Snapshot { since } => {
            let snapshot = TrackerSnapshot {
                schema_version: 1,
                generated_at: Utc::now(),
                active_entry: database.active_entry()?,
                tasks: database.list_tasks()?,
                entries: database.entries_since(parse_since(&since)?)?,
            };
            serde_json::to_writer(std::io::stdout().lock(), &snapshot)?;
            println!();
        }
        Command::Serve { bind } => {
            let token = sync_token()?;
            let automatically_bound = bind.is_none();
            let bind = match bind {
                Some(bind) => bind,
                None => {
                    let ip = TailscaleStatus::load()?.local_ipv4()?;
                    SocketAddr::new(ip.into(), DEFAULT_SYNC_PORT)
                }
            };
            if !automatically_bound && !bind.ip().is_loopback() && token.is_none() {
                bail!(
                    "a manual non-loopback --bind requires TRACKER_SYNC_TOKEN; omit --bind to use the verified Tailscale address automatically"
                );
            }
            if automatically_bound {
                println!("Tracker is available to this tailnet at http://{bind}");
            } else {
                println!("Tracker sync is listening on http://{bind}");
            }
            if token.is_some() {
                println!("Application token authentication is enabled.");
            }
            println!("On another Tailscale device, run `tracker sync`.");
            println!("Press Ctrl-C to stop.");
            drop(database);
            serve(database_path, bind, token).await?;
        }
        Command::Sync { peer: Some(peer) } => {
            let token = sync_token()?;
            sync_one(&mut database, "ad-hoc", &peer, token.as_deref()).await?;
        }
        Command::Sync { peer: None } => {
            let token = sync_token()?;
            let candidates = TailscaleStatus::load()?.online_peers();
            if candidates.is_empty() {
                bail!("no other online Tailscale devices were found");
            }
            println!(
                "Looking for Tracker on {} online Tailscale device(s)…",
                candidates.len()
            );
            let peers =
                discover_tracker_peers(candidates, DEFAULT_SYNC_PORT, token.as_deref()).await?;
            if peers.is_empty() {
                bail!(
                    "no Tracker server was found; run `tracker serve` on an online device such as your homelab, ensure TRACKER_SYNC_TOKEN matches if enabled, then retry"
                );
            }
            sync_all(&mut database, &peers, token.as_deref()).await?;
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

fn normalize_start_tag_shortcuts(arguments: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut normalized = Vec::new();
    let mut in_start_command = false;
    let mut literal_arguments = false;
    let mut option_takes_value = false;

    for argument in arguments {
        if option_takes_value {
            option_takes_value = false;
            normalized.push(argument);
            continue;
        }

        let Some(text) = argument.to_str() else {
            normalized.push(argument);
            continue;
        };

        if !in_start_command {
            in_start_command = text == "start";
            normalized.push(argument);
            continue;
        }

        if literal_arguments {
            normalized.push(argument);
            continue;
        }

        match text {
            "--" => {
                literal_arguments = true;
                normalized.push(argument);
            }
            "--project" | "-p" | "--tag" | "-t" | "--database" => {
                option_takes_value = true;
                normalized.push(argument);
            }
            "--help" | "-h" => normalized.push(argument),
            _ if text.starts_with("--project=")
                || text.starts_with("--tag=")
                || text.starts_with("--database=") =>
            {
                normalized.push(argument);
            }
            _ if text.starts_with("--") && text.len() > 2 => {
                normalized.push(OsString::from("--tag"));
                normalized.push(OsString::from(&text[2..]));
            }
            _ => normalized.push(argument),
        }
    }

    normalized
}

fn sync_token() -> Result<Option<String>> {
    match env::var("TRACKER_SYNC_TOKEN") {
        Ok(token) => Ok(Some(token)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => {
            bail!("TRACKER_SYNC_TOKEN contains invalid Unicode")
        }
    }
}

async fn sync_all(
    database: &mut Database,
    peers: &[ReachablePeer],
    token: Option<&str>,
) -> Result<()> {
    for peer in peers {
        sync_one(database, &peer.name, &peer.url, token).await?;
    }

    // The first pass gathers every peer's records locally. A second pass sends
    // that union back out so all reachable servers converge in one command.
    if peers.len() > 1 {
        for peer in peers {
            sync_with_peer(database, &peer.url, token)
                .await
                .with_context(|| format!("final sync with “{}” failed", peer.name))?;
        }
    }
    println!("All reachable Tracker devices are up to date.");
    Ok(())
}

async fn sync_one(
    database: &mut Database,
    name: &str,
    url: &str,
    token: Option<&str>,
) -> Result<()> {
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
    use clap::error::ErrorKind;

    fn parse(arguments: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(normalize_start_tag_shortcuts(
            arguments.iter().map(OsString::from),
        ))
    }

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

    #[test]
    fn turns_start_shortcuts_into_tags() {
        let cli = parse(&[
            "tracker",
            "start",
            "Chess Study",
            "--chess",
            "--focused-work",
        ])
        .unwrap();
        let Command::Start(arguments) = cli.command else {
            panic!("expected start command");
        };
        assert_eq!(arguments.name, "Chess Study");
        assert_eq!(arguments.tag, ["chess", "focused-work"]);
    }

    #[test]
    fn preserves_regular_start_options() {
        let cli = parse(&[
            "tracker",
            "--database",
            "test.db",
            "start",
            "Read Paper",
            "--project",
            "Cornell",
            "--tag=reading",
            "--cornell",
        ])
        .unwrap();
        let Command::Start(arguments) = cli.command else {
            panic!("expected start command");
        };
        assert_eq!(arguments.project.as_deref(), Some("Cornell"));
        assert_eq!(arguments.tag, ["reading", "cornell"]);
    }

    #[test]
    fn does_not_turn_other_command_options_into_tags() {
        let error = parse(&["tracker", "report", "--unknown"])
            .err()
            .expect("unknown report option should fail");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn preserves_literal_task_names_that_begin_with_a_dash() {
        let cli = parse(&["tracker", "start", "--tag", "safe", "--", "--review"]).unwrap();
        let Command::Start(arguments) = cli.command else {
            panic!("expected start command");
        };
        assert_eq!(arguments.name, "--review");
        assert_eq!(arguments.tag, ["safe"]);
    }
}

//! Command-line interface (`lt`): headless execution.

rust_i18n::i18n!("locales", fallback = "en");

use clap::{Parser, Subcommand};
use lt_core::models::{Profile, RunOptions, RunStatus, Test};
use lt_core::storage::{self, Settings, Workspace};
use lt_engine::engine::{self, RunConfig, RunEvent};
use rust_i18n::t;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Parser, Debug)]
#[command(
    name = "lt",
    version,
    about = "HTTP / SOAP load testing",
    long_about = "Execute Load Testing workspace scenarios without GUI."
)]
struct Cli {
    /// Workspace directory (default: application settings).
    #[arg(long, global = true, env = "LT_WORKSPACE")]
    workspace: Option<PathBuf>,

    /// Language code (en, fr). Default: application settings or en.
    #[arg(long, global = true, env = "LT_LANG")]
    lang: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run a scenario.
    Run {
        /// Scenario ID or name.
        #[arg(long, short)]
        scenario: String,
        /// Profile: smoke, load, stress, spike, soak, tps, breakpoint.
        #[arg(long, short, default_value = "smoke")]
        profile: String,
        /// Number of VUs (or max VUs for throughput profiles).
        #[arg(long)]
        vus: Option<u32>,
        /// Iterations per second (tps / breakpoint profiles).
        #[arg(long)]
        tps: Option<u32>,
        /// Duration (e.g. 30s, 5m, 1h30m).
        #[arg(long)]
        duration: Option<String>,
        /// Ramp-up period (Load profile), e.g. 60s.
        #[arg(long)]
        ramp_up: Option<String>,
        /// Ramp-down period (Load profile), e.g. 30s.
        #[arg(long)]
        ramp_down: Option<String>,
        /// Total iteration count (early stop).
        #[arg(long)]
        iterations: Option<u64>,
        /// Authentication ID to use.
        #[arg(long)]
        auth: Option<String>,
        /// Show only final result.
        #[arg(long)]
        quiet: bool,
        /// Confirm running a scenario marked with real effects.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List workspace scenarios.
    List {
        /// Filter by folder.
        #[arg(long)]
        folder: Option<String>,
    },
    /// Validate scenario consistency.
    Validate {
        /// Limit validation to a scenario.
        #[arg(long)]
        scenario: Option<String>,
    },
    /// Import a legacy workspace (directory containing data/tests.json).
    Import {
        /// Source directory.
        source: PathBuf,
        /// Replace existing data.
        #[arg(long)]
        force: bool,
    },
    /// Show the workspace path in use.
    Where,
}

fn resolve_workspace(explicit: Option<PathBuf>) -> Result<Workspace, String> {
    let settings = Settings::load();
    let root = explicit
        .or_else(|| settings.workspace.clone().map(PathBuf::from))
        .or_else(storage::workspace_from_cwd)
        .unwrap_or_else(storage::default_workspace);
    Workspace::open(&root).map_err(|error| {
        t!(
            "cli.error.workspace_open",
            path = root.display().to_string(),
            error = error.to_string()
        ).to_string()
    })
}

fn find_test<'a>(tests: &'a [Test], needle: &str) -> Option<&'a Test> {
    tests
        .iter()
        .find(|test| test.id == needle)
        .or_else(|| {
            tests
                .iter()
                .find(|test| test.name.eq_ignore_ascii_case(needle))
        })
        .or_else(|| {
            tests
                .iter()
                .find(|test| test.name.to_lowercase().contains(&needle.to_lowercase()))
        })
}

fn format_ms(value: Option<f64>) -> String {
    match value {
        None => "–".to_string(),
        Some(value) if value >= 1000.0 => format!("{:.2} s", value / 1000.0),
        Some(value) => format!("{:.0} ms", value),
    }
}

fn format_pct(value: Option<f64>) -> String {
    match value {
        None => "–".to_string(),
        Some(value) => format!("{:.2} %", value * 100.0),
    }
}

fn format_num(value: Option<f64>) -> String {
    match value {
        None => "–".to_string(),
        Some(value) => {
            let rounded = value.round();
            if (value - rounded).abs() < 0.005 {
                format!("{rounded:.0}")
            } else {
                format!("{value:.2}")
            }
        }
    }
}

fn print_summary(run: &lt_core::models::Run) {
    let Some(summary) = &run.summary else {
        println!("{}", t!("cli.summary.no_summary"));
        return;
    };
    println!();
    println!("  {}", run.test_name);
    println!(
        "  {} · {} · {}",
        run.profile.label(),
        run.status.label(),
        run.duration_ms()
            .map(|ms| lt_core::profiles::format_duration(ms as u64))
            .unwrap_or_else(|| "–".to_string())
    );
    println!();
    println!(
        "  {}",
        t!(
            "cli.summary.requests",
            count = format_num(summary.requests.map(|value| value as f64)),
            rps = format_num(summary.rps)
        )
    );
    println!(
        "  {}",
        t!(
            "cli.summary.transactions",
            count = format_num(summary.iterations.map(|value| value as f64)),
            rate = format_num(summary.iterations_rate)
        )
    );
    println!("  {}", t!("cli.summary.avg", value = format_ms(summary.avg)));
    println!("  {}", t!("cli.summary.median", value = format_ms(summary.med)));
    println!(
        "  {}",
        t!(
            "cli.summary.p90_p95",
            p90 = format_ms(summary.p90),
            p95 = format_ms(summary.p95)
        )
    );
    println!(
        "  {}",
        t!(
            "cli.summary.p99_max",
            p99 = format_ms(summary.p99),
            max = format_ms(summary.max)
        )
    );
    println!(
        "  {}",
        t!(
            "cli.summary.errors_checks",
            errors = format_pct(summary.error_rate),
            checks = format_pct(summary.checks_rate)
        )
    );
    println!("  {}", t!("cli.summary.vus_max", vus = format_num(summary.vus_max)));
    if let Some(dropped) = summary.dropped
        && dropped > 0 {
            println!("  {}", t!("cli.summary.dropped", count = dropped));
        }

    if !summary.thresholds.is_empty() {
        println!();
        println!("  {}", t!("cli.summary.thresholds"));
        for threshold in &summary.thresholds {
            println!(
                "    {} {} {}",
                if threshold.ok { "✓" } else { "✗" },
                threshold.metric,
                threshold.expr
            );
        }
    }
    if !summary.steps.is_empty() {
        println!();
        println!(
            "  {:<28} {:>8} {:>10} {:>10} {:>10} {:>8}",
            t!("cli.table.step"),
            t!("cli.table.requests"),
            t!("cli.table.average"),
            t!("cli.table.p95"),
            t!("cli.table.p99"),
            t!("cli.table.errors")
        );
        for step in &summary.steps {
            println!(
                "  {:<28} {:>8} {:>10} {:>10} {:>10} {:>8}",
                truncate(&step.name, 28),
                format_num(step.count.map(|value| value as f64)),
                format_ms(step.avg),
                format_ms(step.p95),
                format_ms(step.p99),
                format_pct(step.error_rate),
            );
        }
    }
    println!();
    println!("  {}", t!("cli.summary.reports", reports = run.reports.join(", ")));
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let settings = Settings::load();
    let lang = cli.lang.as_deref()
        .or(Some(settings.language.as_str()))
        .unwrap_or("en");
    lt_core::i18n::set_language(lang);
    
    match execute(cli).await {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{}", t!("cli.error.execution_failed", message = message));
            ExitCode::from(2)
        }
    }
}

async fn execute(cli: Cli) -> Result<ExitCode, String> {
    let workspace = resolve_workspace(cli.workspace.clone())?;
    match cli.command {
        Command::Where => {
            println!("{}", workspace.root().display());
            Ok(ExitCode::SUCCESS)
        }
        Command::List { folder } => {
            let tests = workspace.load_tests();
            let auths = workspace.load_auth();
            let mut count = 0;
            for test in &tests {
                if let Some(folder) = &folder
                    && !lt_core::folders::in_folder(test, folder) {
                        continue;
                    }
                count += 1;
                let auth_name = auths
                    .iter()
                    .find(|auth| auth.id == test.auth_id)
                    .map(|auth| auth.name.clone())
                    .unwrap_or_else(|| t!("cli.list.auth_none").to_string());
                println!(
                    "{}  {}  ·  {}  ·  {}  ·  {}",
                    test.id,
                    test.name,
                    t!("cli.list.steps_count", count = test.steps.len()),
                    t!("cli.list.auth_label", auth = auth_name),
                    if test.folder.is_empty() {
                        t!("cli.list.root").to_string()
                    } else {
                        lt_core::folders::folder_label(&test.folder)
                    }
                );
            }
            println!(
                "{}",
                t!(
                    "cli.list.summary",
                    count = count,
                    workspace = workspace.root().display().to_string()
                )
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { scenario } => {
            let tests = workspace.load_tests();
            let selected: Vec<&Test> = match scenario {
                Some(needle) => match find_test(&tests, &needle) {
                    Some(test) => vec![test],
                    None => return Err(t!("cli.error.scenario_not_found", scenario = needle).to_string()),
                },
                None => tests.iter().collect(),
            };
            let mut problems = 0;
            for test in &selected {
                let issues = validate_test(test, &workspace);
                for issue in &issues {
                    println!("✗ {} : {issue}", test.name);
                }
                if issues.is_empty() {
                    println!("✓ {}", test.name);
                }
                problems += issues.len();
            }
            if problems > 0 {
                println!("{}", t!("cli.validate.problem_detected", count = problems));
                Ok(ExitCode::from(1))
            } else {
                println!("{}", t!("cli.validate.scenarios_valid", count = selected.len()));
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Import { source, force } => {
            let report = Workspace::import_from(&source, workspace.root(), force)
                .map_err(|error| error.to_string())?;
            println!(
                "{}",
                t!(
                    "cli.import.complete",
                    files = report.files,
                    runs = report.runs,
                    backfilled = report.backfilled,
                    workspace = report.workspace.display().to_string()
                )
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Run {
            scenario,
            profile,
            vus,
            tps,
            duration,
            ramp_up,
            ramp_down,
            iterations,
            auth,
            quiet,
            yes,
        } => {
            let tests = workspace.load_tests();
            let test = find_test(&tests, &scenario)
                .ok_or_else(|| t!("cli.error.scenario_not_found", scenario = scenario.clone()).to_string())?
                .clone();
            let profile = Profile::parse(&profile)
                .ok_or_else(|| t!("cli.error.profile_unknown", profile = profile.clone()).to_string())?;
            if test.dangerous && !yes {
                return Err(t!("cli.error.dangerous_confirm", name = test.name.clone()).to_string());
            }
            if let Some(duration) = &duration {
                lt_core::profiles::parse_duration_ms(duration)
                    .map_err(|error| t!("cli.error.duration_invalid", error = error.to_string()).to_string())?;
            }
            if let Some(ramp_up) = &ramp_up {
                lt_core::profiles::parse_duration_ms(ramp_up)
                    .map_err(|error| t!("cli.error.ramp_up_invalid", error = error.to_string()).to_string())?;
            }
            if let Some(ramp_down) = &ramp_down {
                lt_core::profiles::parse_duration_ms(ramp_down)
                    .map_err(|error| t!("cli.error.ramp_down_invalid", error = error.to_string()).to_string())?;
            }
            let auth = auth
                .as_ref()
                .and_then(|id| {
                    workspace
                        .load_auth()
                        .into_iter()
                        .find(|entry| &entry.id == id || entry.name.eq_ignore_ascii_case(id))
                })
                .or_else(|| {
                    workspace
                        .load_auth()
                        .into_iter()
                        .find(|entry| entry.id == test.auth_id)
                });

            let options = RunOptions {
                vus,
                tps,
                duration,
                ramp_up,
                ramp_down,
                iterations,
                ..RunOptions::default()
            };
            let mut config = RunConfig::new(workspace.clone(), test, profile);
            config.options = options;
            config.auth = auth;

            let stop = CancellationToken::new();
            let signal = stop.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    eprintln!("{}", t!("cli.run.stop_requested"));
                    signal.cancel();
                }
            });

            let reporter: Option<engine::Reporter> = if quiet {
                None
            } else {
                Some(Arc::new(|event: RunEvent| if let RunEvent::Log(message) = event { println!("{message}") }))
            };

            let run = engine::execute(config, reporter, stop)
                .await
                .map_err(|error| error.to_string())?;
            print_summary(&run);
            Ok(match run.status {
                RunStatus::Passed => ExitCode::SUCCESS,
                RunStatus::Stopped => ExitCode::from(130),
                RunStatus::Error => ExitCode::from(2),
                _ => ExitCode::from(1),
            })
        }
    }
}

fn validate_test(test: &Test, workspace: &Workspace) -> Vec<String> {
    let mut issues = Vec::new();
    if test.name.trim().is_empty() {
        issues.push(t!("cli.validate.name_empty").to_string());
    }
    if test.steps.is_empty() {
        issues.push(t!("cli.validate.no_steps").to_string());
    }
    let base = test.base_url.trim();
    if base.is_empty() || base == "https://" || base == "http://" {
        issues.push(t!("cli.validate.base_url_missing").to_string());
    } else if !(base.starts_with("http://") || base.starts_with("https://"))
        || url::Url::parse(base).is_err()
    {
        issues.push(t!("cli.validate.base_url_invalid", url = base).to_string());
    }
    if !test.auth_id.is_empty() && !workspace.load_auth().iter().any(|auth| auth.id == test.auth_id)
    {
        issues.push(t!("cli.validate.auth_not_found").to_string());
    }
    if test.thresholds.error_rate.is_some_and(|value| !(0.0..=1.0).contains(&value)) {
        issues.push(t!("cli.validate.threshold_error_rate").to_string());
    }
    if test
        .thresholds
        .checks_rate
        .is_some_and(|value| !(0.0..=1.0).contains(&value))
    {
        issues.push(t!("cli.validate.threshold_checks_rate").to_string());
    }
    for (index, step) in test.steps.iter().enumerate() {
        let name = step.effective_name(index);
        if step.path.trim().is_empty() {
            issues.push(t!("cli.validate.step_path_empty", step = name.clone()).to_string());
        }
        if step.expected_statuses().is_empty() {
            issues.push(t!("cli.validate.step_status_invalid", step = name.clone()).to_string());
        }
        if step.is_soap() && step.body.trim().is_empty() {
            issues.push(t!("cli.validate.step_soap_body_empty", step = name.clone()).to_string());
        }
    }
    issues
}

//! Orchestration : sélection de l'exécuteur, boucle des VUs, arrêt automatique
//! sur seuil, série temporelle, rapports et historique.

use crate::auth::AuthProvider;
use crate::http;
use crate::metrics::Metrics;
use crate::EngineError;
use lt_core::profiles::{ExecutorPlan, Stage, ThresholdKind, ThresholdRule};
use lt_core::{
    Auth, Profile, Run, RunOptions, RunSeries, RunStatus, SeriesPoint, Test, Workspace,
};
use rust_i18n::t;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

    /// Événements émis pendant (et à la fin) d'une exécution.
#[derive(Debug, Clone)]
pub enum RunEvent {
    Started { run: Run },
    Log(String),
    Sample(SeriesPoint),
    Finished { run: Run },
    /// Erreur fatale du moteur (aucune exécution n'a pu démarrer).
    Fatal(String),
}

pub type Reporter = Arc<dyn Fn(RunEvent) + Send + Sync>;

/// Journal d'exécution : fichier `<run>.log` + remontée vers l'interface.
#[derive(Clone)]
pub struct Logger {
    reporter: Option<Reporter>,
    file: Arc<Mutex<Option<std::fs::File>>>,
}

impl Logger {
    pub fn new(reporter: Option<Reporter>, path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&path)?;
        Ok(Self {
            reporter,
            file: Arc::new(Mutex::new(Some(file))),
        })
    }

    pub fn log(&self, message: impl Into<String>) {
        let message = message.into();
        let line = format!(
            "[{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            message
        );
        if let Some(file) = self.file.lock().as_mut() {
            use std::io::Write as _;
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
        if let Some(reporter) = &self.reporter {
            reporter(RunEvent::Log(message));
        }
    }
}

/// Paramètres d'une exécution.
#[derive(Clone, Debug)]
pub struct RunConfig {
    pub workspace: Workspace,
    pub test: Test,
    pub auth: Option<Auth>,
    pub profile: Profile,
    pub options: RunOptions,
    /// Nom de fichier (sans extension) ; généré automatiquement si absent.
    pub run_name: Option<String>,
}

impl RunConfig {
    pub fn new(workspace: Workspace, test: Test, profile: Profile) -> Self {
        Self {
            workspace,
            test,
            auth: None,
            profile,
            options: RunOptions::default(),
            run_name: None,
        }
    }
}

struct Ctx {
    test: Test,
    client: reqwest::Client,
    auth: Option<Arc<AuthProvider>>,
    metrics: Arc<Metrics>,
    max_iterations: Option<u64>,
}

impl Ctx {
    fn iterations_exhausted(&self) -> bool {
        match self.max_iterations {
            Some(max) => self.metrics.iterations() >= max,
            None => false,
        }
    }
}

fn now_ms() -> i64 {
    lt_core::template::now_ms() as i64
}

fn persist(workspace: &Workspace, run: &Run) -> Result<(), EngineError> {
    let mut runs = workspace.load_runs();
    match runs.iter_mut().find(|existing| existing.id == run.id) {
        Some(existing) => *existing = run.clone(),
        None => runs.insert(0, run.clone()),
    }
    workspace.save_runs(&runs)?;
    Ok(())
}

fn threshold_summary(rules: &[ThresholdRule]) -> String {
    rules
        .iter()
        .map(|rule| format!("{} {}", rule.metric, rule.expr))
        .collect::<Vec<_>>()
        .join(", ")
}

fn describe_plan(plan: &ExecutorPlan) -> String {
    match plan {
        ExecutorPlan::ConstantVus { vus, duration_ms } => {
            let duration = lt_core::profiles::format_duration(*duration_ms);
            t!(
                "engine.plan.constant_vus",
                vus = vus,
                duration = duration
            )
            .to_string()
        }
        ExecutorPlan::RampingVus { stages, .. } => {
            let steps = stages
                .iter()
                .map(|stage| {
                    format!(
                        "{} → {} VUs",
                        lt_core::profiles::format_duration(stage.duration_ms),
                        stage.target.round() as i64
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            t!("engine.plan.ramping_vus", steps = steps).to_string()
        }
        ExecutorPlan::ConstantArrival {
            rate, duration_ms, ..
        } => {
            let duration = lt_core::profiles::format_duration(*duration_ms);
            t!(
                "engine.plan.constant_arrival",
                rate = rate,
                duration = duration
            )
            .to_string()
        }
        ExecutorPlan::RampingArrival { stages, .. } => {
            let steps = stages
                .iter()
                .map(|stage| {
                    format!(
                        "{} → {}/s",
                        lt_core::profiles::format_duration(stage.duration_ms),
                        stage.target.round() as i64
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            t!("engine.plan.ramping_arrival", steps = steps).to_string()
        }
    }
}

/// Lance une exécution et renvoie l'entrée d'historique finale.
pub async fn execute(
    config: RunConfig,
    reporter: Option<Reporter>,
    stop_token: CancellationToken,
) -> Result<Run, EngineError> {
    let run_name = config.run_name.clone().unwrap_or_else(|| {
        lt_core::storage::run_name(&config.test.name, config.profile.as_str())
    });
    let logger = Logger::new(
        reporter.clone(),
        config.workspace.run_log_path(&run_name),
    )?;

    let plan = lt_core::profiles::build_plan(config.profile, &config.options)
        .map_err(EngineError::Message)?;
    let rules = lt_core::profiles::build_thresholds(&config.test, config.profile);

    let mut run = Run {
        id: lt_core::storage::new_id(),
        name: run_name.clone(),
        test_id: config.test.id.clone(),
        test_name: config.test.name.clone(),
        profile: config.profile,
        options: config.options.clone(),
        status: RunStatus::Running,
        started_at: now_ms(),
        ..Run::default()
    };
    persist(&config.workspace, &run)?;
    if let Some(reporter) = &reporter {
        reporter(RunEvent::Started { run: run.clone() });
    }

    logger.log(t!("engine.log.scenario", name = config.test.name).to_string());
    logger.log(t!("engine.log.target", url = config.test.base_url).to_string());
    logger.log(t!("engine.log.profile", label = config.profile.label()).to_string());
    logger.log(t!("engine.log.executor", plan = describe_plan(&plan)).to_string());
    if !rules.is_empty() {
        logger.log(t!("engine.log.thresholds", rules = threshold_summary(&rules)).to_string());
    }
    if config.test.insecure_tls {
        logger.log(t!("engine.log.insecure_tls").to_string());
    }
    if let Err(error) = config
        .workspace
        .save_run_config(&run_name, &config.test, config.auth.as_ref())
    {
        logger.log(t!("engine.log.config_write_failed", error = error).to_string());
    }

    // Client HTTP et authentification.
    let auth = match AuthProvider::prepare(config.auth.as_ref(), config.test.insecure_tls).await {
        Ok(auth) => auth,
        Err(error) => {
            let mut failed = run.clone();
            failed.status = RunStatus::Error;
            failed.ended_at = Some(now_ms());
            failed.exit_code = Some(2);
            failed.error = Some(error.to_string());
            persist(&config.workspace, &failed)?;
            logger.log(t!("engine.log.auth_error", error = error).to_string());
            if let Some(reporter) = &reporter {
                reporter(RunEvent::Finished {
                    run: failed.clone(),
                });
            }
            return Ok(failed);
        }
    };
    if let Some(auth) = &auth {
        logger.log(t!("engine.log.auth", desc = auth.description()).to_string());
    }
    let client = http::build_client(
        config.test.insecure_tls,
        Duration::from_secs(60),
    )?;

    let step_names: Vec<String> = config
        .test
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| step.effective_name(index))
        .collect();
    let labels: Vec<Vec<String>> = config
        .test
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| http::check_labels(step, index))
        .collect();
    let metrics = Arc::new(Metrics::new(&step_names, &labels));

    let ctx = Arc::new(Ctx {
        test: config.test.clone(),
        client,
        auth,
        metrics: Arc::clone(&metrics),
        max_iterations: config.options.iterations,
    });

    let series: Arc<Mutex<Vec<SeriesPoint>>> = Arc::new(Mutex::new(Vec::new()));
    let stop = CancellationToken::new();

    // Échantillonnage toutes les secondes.
    let ticker = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        let series = Arc::clone(&series);
        let reporter = reporter.clone();
        let stop = stop.clone();
        async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = interval.tick() => {
                        let point = metrics.snapshot();
                        series.lock().push(point.clone());
                        if let Some(reporter) = &reporter {
                            reporter(RunEvent::Sample(point));
                        }
                    }
                }
            }
        }
    });

    if let Some(auth) = &ctx.auth {
        auth.start_refresh(stop.clone(), logger.clone());
    }

    let executor = tokio::spawn(run_plan(Arc::clone(&ctx), plan.clone(), stop.clone()));

    // Coordination : durée, nombre d'itérations, arrêt manuel, seuil bloquant.
    let duration_ms = plan.total_duration_ms();
    let started = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    let mut stopped_externally = false;
    let mut aborted = false;
    loop {
        tokio::select! {
            _ = stop_token.cancelled() => {
                stopped_externally = true;
                break;
            }
            _ = tick.tick() => {
                let elapsed = started.elapsed().as_millis() as u64;
                if duration_ms > 0 && elapsed >= duration_ms {
                    break;
                }
                if ctx.iterations_exhausted() {
                    break;
                }
                if let Some(reason) = threshold_abort(&rules, &metrics, elapsed) {
                    aborted = true;
                    logger.log(t!("engine.log.auto_stop", reason = reason).to_string());
                    break;
                }
            }
        }
    }
    stop.cancel();

    let _ = tokio::time::timeout(Duration::from_secs(6), executor).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), ticker).await;

    let final_point = metrics.snapshot();
    series.lock().push(final_point);

    let points = std::mem::take(&mut *series.lock());
    let series = RunSeries {
        run_id: run.id.clone(),
        points,
    };
    let summary = metrics.summary(&rules);

    let status = if stopped_externally {
        RunStatus::Stopped
    } else if aborted {
        RunStatus::Failed
    } else if summary.thresholds_ok() {
        RunStatus::Passed
    } else {
        RunStatus::Failed
    };

    run.status = status;
    run.ended_at = Some(now_ms());
    run.exit_code = Some(match status {
        RunStatus::Passed => 0,
        RunStatus::Stopped => 130,
        RunStatus::Error => 2,
        _ => 1,
    });
    run.summary = Some(summary.clone());

    // Arrêt manuel : on conserve le résumé en mémoire / historique, mais on
    // n'écrit pas les rapports HTML/PDF/JSON (pas de « téléchargement » auto).
    if status == RunStatus::Stopped {
        logger.log(t!("engine.log.reports_skipped_stopped").to_string());
    } else {
        match lt_core::report::write_reports(&config.workspace, &run, &summary, &series) {
            Ok(reports) => run.reports = reports,
            Err(error) => {
                logger.log(t!("engine.log.reports_failed", error = error).to_string());
                run.error = Some(error.to_string());
            }
        }
    }
    persist(&config.workspace, &run)?;

    let errors = format!("{:.2}", summary.error_rate.unwrap_or(0.0) * 100.0);
    logger.log(
        t!(
            "engine.log.finished",
            status = status.label(),
            requests = summary.requests.unwrap_or(0),
            p95 = summary.p95.unwrap_or(0.0).round() as i64,
            errors = errors
        )
        .to_string(),
    );
    for threshold in summary.failed_thresholds() {
        logger.log(
            t!(
                "engine.log.threshold_failed",
                metric = threshold.metric,
                expr = threshold.expr
            )
            .to_string(),
        );
    }
    if let Some(reporter) = &reporter {
        reporter(RunEvent::Finished { run: run.clone() });
    }
    Ok(run)
}

/// Renvoie la raison d'un arrêt automatique, si un seuil bloquant est cassé.
fn threshold_abort(rules: &[ThresholdRule], metrics: &Metrics, elapsed_ms: u64) -> Option<String> {
    let blocking: Vec<&ThresholdRule> = rules.iter().filter(|rule| rule.abort_on_fail).collect();
    if blocking.is_empty() {
        return None;
    }
    let (p95, error_rate, _) = metrics.current();
    for rule in blocking {
        let delay = rule
            .delay_abort
            .map(|delay| delay.as_millis() as u64)
            .unwrap_or(0);
        if elapsed_ms < delay {
            continue;
        }
        let broken = match rule.kind {
            ThresholdKind::DurationP95 => p95 >= rule.limit,
            ThresholdKind::ErrorRate => error_rate >= rule.limit,
            _ => false,
        };
        if broken {
            return Some(format!("{} {}", rule.metric, rule.expr));
        }
    }
    None
}

async fn run_plan(ctx: Arc<Ctx>, plan: ExecutorPlan, stop: CancellationToken) {
    match plan {
        ExecutorPlan::ConstantVus { vus, .. } => run_constant_vus(ctx, vus, stop).await,
        ExecutorPlan::RampingVus {
            start_vus, stages, ..
        } => run_ramping_vus(ctx, start_vus, stages, stop).await,
        ExecutorPlan::ConstantArrival {
            rate,
            max_vus,
            pre_allocated_vus,
            ..
        } => {
            let executor = ArrivalExecutor::new(
                ctx,
                vec![Stage {
                    duration_ms: 0,
                    target: rate,
                }],
                rate,
                pre_allocated_vus,
                max_vus,
            );
            executor.run(stop).await;
        }
        ExecutorPlan::RampingArrival {
            start_rate,
            stages,
            max_vus,
            pre_allocated_vus,
        } => {
            let executor =
                ArrivalExecutor::new(ctx, stages, start_rate, pre_allocated_vus, max_vus);
            executor.run(stop).await;
        }
    }
}

async fn run_iteration(ctx: &Ctx, vu: u64, iter: u64) {
    let vars = ctx.test.placeholder_vars(vu, iter);
    for (index, step) in ctx.test.steps.iter().enumerate() {
        let execution = http::execute_step(
            &ctx.client,
            &ctx.test,
            step,
            index,
            &vars,
            vu,
            iter,
            ctx.auth.as_deref(),
        )
        .await;
        let failed = execution.response.error.is_some() || !execution.ok;
        ctx.metrics.record_request(
            index,
            (execution.response.duration_ms * 1000.0).round().max(1.0) as u64,
            failed,
        );
        let results: Vec<bool> = execution.checks.iter().map(|check| check.ok).collect();
        ctx.metrics.record_checks(index, &results);
        if failed && step.stop_on_failure {
            break;
        }
    }
    ctx.metrics.iteration();
}

async fn think_time(ctx: &Ctx, stop: &CancellationToken) {
    let seconds = ctx.test.think_time.max(0.0);
    if seconds <= 0.0 {
        return;
    }
    tokio::select! {
        _ = stop.cancelled() => {}
        _ = tokio::time::sleep(Duration::from_secs_f64(seconds)) => {}
    }
}

async fn run_constant_vus(ctx: Arc<Ctx>, vus: u32, stop: CancellationToken) {
    let mut handles = Vec::new();
    for vu in 0..vus {
        let ctx = Arc::clone(&ctx);
        let stop = stop.clone();
        handles.push(tokio::spawn(async move {
            ctx.metrics.enter_vu();
            let mut iter = 0u64;
            loop {
                if stop.is_cancelled() || ctx.iterations_exhausted() {
                    break;
                }
                run_iteration(&ctx, vu as u64, iter).await;
                iter += 1;
                if ctx.iterations_exhausted() {
                    break;
                }
                think_time(&ctx, &stop).await;
            }
            ctx.metrics.leave_vu();
        }));
    }
    for handle in handles {
        let _ = handle.await;
    }
}

async fn run_ramping_vus(
    ctx: Arc<Ctx>,
    start_vus: u32,
    stages: Vec<Stage>,
    stop: CancellationToken,
) {
    let peak = stages
        .iter()
        .map(|stage| stage.target)
        .fold(start_vus as f64, f64::max)
        .ceil()
        .max(1.0) as u32;
    let target = Arc::new(AtomicU64::new(start_vus as u64));

    let mut handles = Vec::new();
    for vu in 0..peak {
        let ctx = Arc::clone(&ctx);
        let stop = stop.clone();
        let target = Arc::clone(&target);
        handles.push(tokio::spawn(async move {
            let mut active = false;
            let mut iter = 0u64;
            loop {
                if stop.is_cancelled() || ctx.iterations_exhausted() {
                    break;
                }
                let should_run = (vu as u64) < target.load(Ordering::Relaxed);
                if should_run && !active {
                    ctx.metrics.enter_vu();
                    active = true;
                } else if !should_run && active {
                    ctx.metrics.leave_vu();
                    active = false;
                }
                if !should_run {
                    tokio::select! {
                        _ = stop.cancelled() => break,
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                    }
                    continue;
                }
                run_iteration(&ctx, vu as u64, iter).await;
                iter += 1;
                think_time(&ctx, &stop).await;
            }
            if active {
                ctx.metrics.leave_vu();
            }
        }));
    }

    let updater = {
        let target = Arc::clone(&target);
        let stop = stop.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = interval.tick() => {
                        let elapsed = started.elapsed().as_millis() as u64;
                        let value = interpolate(&stages, start_vus as f64, elapsed);
                        target.store(value.round().max(0.0) as u64, Ordering::Relaxed);
                    }
                }
            }
        })
    };

    for handle in handles {
        let _ = handle.await;
    }
    let _ = updater.await;
}

/// Valeur interpolée linéairement dans une suite de paliers.
fn interpolate(stages: &[Stage], start: f64, elapsed_ms: u64) -> f64 {
    let mut offset = 0u64;
    let mut from = start;
    for stage in stages {
        if elapsed_ms < offset + stage.duration_ms {
            let progress = if stage.duration_ms == 0 {
                1.0
            } else {
                (elapsed_ms.saturating_sub(offset)) as f64 / stage.duration_ms as f64
            };
            return from + (stage.target - from) * progress.clamp(0.0, 1.0);
        }
        offset += stage.duration_ms;
        from = stage.target;
    }
    from
}

struct ArrivalExecutor {
    ctx: Arc<Ctx>,
    stages: Vec<Stage>,
    start_rate: f64,
    semaphore: Arc<Semaphore>,
    counter: AtomicU64,
}

impl ArrivalExecutor {
    fn new(
        ctx: Arc<Ctx>,
        stages: Vec<Stage>,
        start_rate: f64,
        pre_allocated: u32,
        max_vus: u32,
    ) -> Self {
        let limit = max_vus.max(pre_allocated).max(1) as usize;
        Self {
            ctx,
            stages,
            start_rate,
            semaphore: Arc::new(Semaphore::new(limit)),
            counter: AtomicU64::new(0),
        }
    }

    async fn run(self, stop: CancellationToken) {
        let mut set: JoinSet<()> = JoinSet::new();
        let mut ticker = tokio::time::interval(Duration::from_millis(20));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let started = Instant::now();
        let mut tokens = 0.0f64;

        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = ticker.tick() => {
                    if self.ctx.iterations_exhausted() {
                        continue;
                    }
                    let elapsed = started.elapsed().as_millis() as u64;
                    let rate = interpolate(&self.stages, self.start_rate, elapsed);
                    tokens += (rate * 0.02).max(0.0);
                    while tokens >= 1.0 {
                        tokens -= 1.0;
                        match Arc::clone(&self.semaphore).try_acquire_owned() {
                            Ok(permit) => {
                                let ctx = Arc::clone(&self.ctx);
                                let vu = self.counter.fetch_add(1, Ordering::Relaxed);
                                set.spawn(async move {
                                    let _permit = permit;
                                    ctx.metrics.enter_vu();
                                    run_iteration(&ctx, vu, 0).await;
                                    ctx.metrics.leave_vu();
                                });
                            }
                            Err(_) => {
                                self.ctx.metrics.dropped_iteration();
                                break;
                            }
                        }
                    }
                    while set.try_join_next().is_some() {}
                }
            }
        }

        let drain = async {
            while set.join_next().await.is_some() {}
        };
        if tokio::time::timeout(Duration::from_secs(3), drain)
            .await
            .is_err()
        {
            set.abort_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_follows_stages() {
        let stages = vec![
            Stage {
                duration_ms: 1000,
                target: 10.0,
            },
            Stage {
                duration_ms: 1000,
                target: 20.0,
            },
        ];
        assert!((interpolate(&stages, 0.0, 0) - 0.0).abs() < 0.001);
        assert!((interpolate(&stages, 0.0, 500) - 5.0).abs() < 0.001);
        assert!((interpolate(&stages, 0.0, 1000) - 10.0).abs() < 0.001);
        assert!((interpolate(&stages, 0.0, 1500) - 15.0).abs() < 0.001);
        assert!((interpolate(&stages, 0.0, 5000) - 20.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn runs_a_smoke_test_against_a_local_target() {
        use tokio::io::AsyncWriteExt as _;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1024];
                    let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await;
                    let body = "{\"ok\":true}";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let dir = std::env::temp_dir().join(format!("lt-engine-{}", lt_core::storage::new_id()));
        let workspace = Workspace::open(&dir).unwrap();

        let test = Test {
            name: "local".to_string(),
            base_url: format!("http://{address}"),
            think_time: 0.0,
            steps: vec![lt_core::Step {
                name: "ping".to_string(),
                path: "/".to_string(),
                ..lt_core::Step::default()
            }],
            ..Test::default()
        };

        let mut config = RunConfig::new(workspace.clone(), test, Profile::Smoke);
        config.options.vus = Some(1);
        config.options.duration = Some("1s".to_string());

        let run = execute(config, None, CancellationToken::new()).await.unwrap();
        assert_eq!(run.status, RunStatus::Passed);
        assert!(run.summary.as_ref().unwrap().requests.unwrap_or(0) > 0);
        assert!(!run.reports.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

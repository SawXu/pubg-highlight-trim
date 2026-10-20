//! Full application lifecycle. A source probe is never represented as a completed scan.
use crate::{config::Config, output_files::OutputPaths, source_files};
#[cfg(not(feature = "onnx"))]
pub fn run(c: Config) -> Result<u8, String> {
    let inputs = source_files::select(&c)?;
    OutputPaths::plan(&c, &inputs).map_err(|e| e.to_string())?;
    Err("This build has no ONNX OCR support; use the production Rust release or build with --features onnx. Media probing is not a successful scan.".into())
}
#[cfg(feature = "onnx")]
pub fn run(c: Config) -> Result<u8, String> {
    production::run(c)
}

#[cfg(feature = "onnx")]
mod production {
    use super::*;
    use crate::{
        media::{FfmpegTools, MediaError},
        persistent_cache,
        planning::{self, Detection, PlanningConfig},
        reports::{self, Phase, PhaseTimings, Record, Timing},
        scan_adapter::{self, EngineSettings},
        scanner,
        source_files::Source,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use std::{
        collections::{BTreeMap, HashMap},
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    #[derive(Clone, Default, Serialize, Deserialize)]
    struct ScanResult {
        #[serde(default)]
        provider: Option<Value>,
        duration: f64,
        probe_seconds: f64,
        source_seconds: f64,
        detections: Option<Vec<Detection>>,
        error: Option<String>,
        #[serde(default)]
        current_stats: Option<scanner::OcrRuntimeStats>,
        #[serde(default)]
        current_work: Option<scanner::ScanWork>,
    }
    /// Historical event-time snapshots only, not completed scan measurements.
    /// Old cache payloads cannot recover work after the last accepted event.
    #[derive(Default, Serialize)]
    struct HistoricalSnapshotTelemetry {
        sources: usize,
        sources_without_detection_telemetry: usize,
        probe_seconds_sum: f64,
        detect_seconds_sum: f64,
        ocr_predict_seconds_sum: f64,
        ocr_frame_seconds_sum: f64,
        refine_ocr_seconds_sum: f64,
        assist_ocr_seconds_sum: f64,
        ocr_requests: u64,
        ocr_successes: u64,
        ocr_skipped: u64,
        ocr_cache_hits: u64,
        refine_frames_snapshot: u64,
        assist_budget_used_snapshot: u64,
    }
    impl HistoricalSnapshotTelemetry {
        fn add(&mut self, r: &ScanResult) {
            self.sources += 1;
            let Some(d) = &r.detections else {
                self.sources_without_detection_telemetry += 1;
                return;
            };
            if d.is_empty() {
                self.sources_without_detection_telemetry += 1;
            }
            macro_rules! seconds {
                ($out:ident, $field:ident) => {
                    self.$out += d.iter().map(|d| d.$field).fold(0.0, f64::max);
                };
            }
            macro_rules! count {
                ($out:ident, $field:ident) => {
                    self.$out += d.iter().map(|d| d.$field as u64).max().unwrap_or(0);
                };
            }
            seconds!(detect_seconds_sum, detect_seconds);
            seconds!(ocr_predict_seconds_sum, ocr_seconds);
            seconds!(ocr_frame_seconds_sum, ocr_frame_seconds);
            seconds!(refine_ocr_seconds_sum, refine_ocr_seconds);
            seconds!(assist_ocr_seconds_sum, assist_ocr_seconds);
            count!(ocr_requests, ocr_requests);
            count!(ocr_successes, ocr_successes);
            count!(ocr_skipped, ocr_skipped);
            count!(ocr_cache_hits, ocr_cache_hits);
            count!(refine_frames_snapshot, ocr_refine_frames);
            count!(assist_budget_used_snapshot, assist_budget_used);
        }
    }
    #[derive(Serialize, Deserialize)]
    struct CachedScan {
        detections: Vec<Detection>,
        provider: Option<Value>,
    }
    struct PlannedTrim {
        sequence: usize,
        clip: planning::ClipPlan,
        output: Option<PathBuf>,
    }
    struct EncodedTrim {
        staged: crate::media_output::MediaOutput,
        encoder: String,
        encode_seconds: f64,
        probe_seconds: f64,
        started: Instant,
    }
    // Only this bounded window owns running processes or staged outputs. On
    // error/cancellation join every worker before their staging guards are dropped.
    struct TrimQueue<I: Iterator<Item = (PathBuf, PathBuf, f64, f64)>> {
        jobs: I,
        pending: std::collections::VecDeque<std::thread::JoinHandle<Result<EncodedTrim, String>>>,
        encoder: crate::media::TrimEncoder,
        tools: FfmpegTools,
        inputs: std::sync::Arc<Vec<PathBuf>>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        failure: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }
    impl<I: Iterator<Item = (PathBuf, PathBuf, f64, f64)>> TrimQueue<I> {
        fn submit(&mut self) -> Result<(), String> {
            crate::cancellation::check()?;
            let Some((source, output, start, length)) = self.jobs.next() else {
                return Ok(());
            };
            let mut encoder = self.encoder.clone();
            let tools = self.tools.clone();
            let inputs = self.inputs.clone();
            let stop = self.stop.clone();
            let failure = self.failure.clone();
            self.pending.push_back(std::thread::spawn(move || {
                let result: Result<EncodedTrim, String> =
                    crate::media::with_task_stop(stop.clone(), || {
                        crate::cancellation::check()?;
                        let started = Instant::now();
                        let staged =
                            crate::media_output::MediaOutput::reserve(&output, false, &inputs)
                                .map_err(|e| e.to_string())?;
                        let encode_started = Instant::now();
                        let encoder = encoder
                            .trim_clip(&source, staged.temporary(), start, length)
                            .map_err(|e| e.to_string())?
                            .to_owned();
                        let encode_seconds = encode_started.elapsed().as_secs_f64();
                        let probe_started = Instant::now();
                        tools
                            .duration_sec(staged.temporary())
                            .map_err(|e| e.to_string())?;
                        Ok(EncodedTrim {
                            staged,
                            encoder,
                            encode_seconds,
                            probe_seconds: probe_started.elapsed().as_secs_f64(),
                            started,
                        })
                    });
                if let Err(error) = &result {
                    let mut first = failure.lock().unwrap();
                    if first.is_none() {
                        *first = Some(error.clone());
                    }
                    stop.store(true, Ordering::Release);
                }
                result
            }));
            Ok(())
        }
        fn next(&mut self) -> Result<EncodedTrim, String> {
            let result = self
                .pending
                .pop_front()
                .ok_or("missing trim worker")?
                .join()
                .map_err(|_| "trim worker panicked".to_owned())?;
            if let Some(error) = self.failure.lock().unwrap().as_ref() {
                return Err(error.clone());
            }
            result
        }
    }
    impl<I: Iterator<Item = (PathBuf, PathBuf, f64, f64)>> Drop for TrimQueue<I> {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            for worker in self.pending.drain(..) {
                // Result owns the staging guard, including successfully encoded
                // clips that must never be published after another task failed.
                let _ = worker.join();
            }
        }
    }
    struct ScanJob {
        source: Source,
        config: scanner::OcrConfig,
        candidates: Vec<f64>,
        cache_key: Option<String>,
    }
    fn progress(current: usize, total: usize, workers: usize) {
        println!(
            "PROGRESS {}",
            json!({"phase":"scan","current":current,"total":total,"workers":workers})
        );
    }
    /// Emit phase start: current=0, workers=N (N=0 means no work/phase skipped).
    fn progress_phase_start(phase: &str, total: usize, workers: usize) {
        println!(
            "PROGRESS {}",
            json!({"phase":phase,"event":"start","current":0,"total":total,"workers":workers})
        );
    }
    fn progress_phase_current(phase: &str, current: usize, total: usize, workers: usize) {
        println!(
            "PROGRESS {}",
            json!({"phase":phase,"current":current,"total":total,"workers":workers})
        );
    }
    fn progress_phase_done(phase: &str, total: usize, workers: usize) -> Result<(), String> {
        crate::cancellation::check()?;
        println!(
            "PROGRESS {}",
            json!({"phase":phase,"event":"done","current":total,"total":total,"workers":workers})
        );
        Ok(())
    }
    fn progress_phase_skipped(phase: &str, reason: &str, planned: usize) {
        println!(
            "PROGRESS {}",
            json!({"phase":phase,"event":"skipped","current":0,"total":0,"workers":0,"reason":reason,"planned":planned})
        );
    }
    fn available_memory_gb() -> Option<f64> {
        #[cfg(windows)]
        {
            #[repr(C)]
            struct MemoryStatus {
                length: u32,
                load: u32,
                total: u64,
                available: u64,
                total_page: u64,
                available_page: u64,
                total_virtual: u64,
                available_virtual: u64,
                extended: u64,
            }
            #[link(name = "kernel32")]
            extern "system" {
                fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
            }
            let mut m = MemoryStatus {
                length: std::mem::size_of::<MemoryStatus>() as u32,
                load: 0,
                total: 0,
                available: 0,
                total_page: 0,
                available_page: 0,
                total_virtual: 0,
                available_virtual: 0,
                extended: 0,
            };
            if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 {
                Some(m.available as f64 / 1073741824.0)
            } else {
                None
            }
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn workers(c: &Config, count: usize) -> usize {
        if c.jobs != "auto" {
            return c.jobs.parse().unwrap();
        }
        if count < 2
            || std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                < 8
            || available_memory_gb().is_some_and(|m| m < 12.0)
        {
            1
        } else {
            2
        }
    }
    fn is_unreadable(e: &MediaError) -> bool {
        matches!(e, MediaError::VideoUnreadable(_))
    }
    fn scan(
        job: &ScanJob,
        tools: &FfmpegTools,
        engine: &crate::OcrEngine<crate::PpOcrRecognizer>,
    ) -> Result<ScanResult, String> {
        let started = Instant::now();
        let duration = match tools.duration_sec(&job.source.path) {
            Ok(d) => d,
            Err(e) if is_unreadable(&e) => {
                return Ok(ScanResult {
                    provider: None,
                    duration: 0.0,
                    source_seconds: 0.0,
                    probe_seconds: started.elapsed().as_secs_f64(),
                    detections: None,
                    error: Some(format!(
                        "ffprobe duration probe failed for {}: {e}",
                        job.source.path.display()
                    )),
                    ..ScanResult::default()
                })
            }
            Err(e) => return Err(e.to_string()),
        };
        let probe_seconds = started.elapsed().as_secs_f64();
        let mut backend = match scan_adapter::backend(&job.source, tools, engine) {
            Ok(b) => b,
            Err(e) if is_unreadable(&e) => {
                return Ok(ScanResult {
                    provider: None,
                    duration,
                    source_seconds: 0.0,
                    probe_seconds,
                    detections: None,
                    error: Some(e.to_string()),
                    ..ScanResult::default()
                })
            }
            Err(e) => return Err(e.to_string()),
        };
        let rules = crate::events::language_profile(job.source.language.code())?;
        match scanner::detect_events(&mut backend, &rules, duration, &job.candidates, &job.config) {
            Ok(report) => Ok(ScanResult {
                provider: None,
                duration,
                probe_seconds,
                source_seconds: 0.0,
                detections: Some(scan_adapter::planning_detections(report.detections)?),
                current_stats: Some(report.stats),
                current_work: Some(report.work),
                error: None,
            }),
            Err(e)
                if scanner::ScanBackend::failure_kind(&backend)
                    == scanner::ScanFailureKind::SourceUnreadable =>
            {
                Ok(ScanResult {
                    provider: None,
                    duration,
                    source_seconds: 0.0,
                    probe_seconds,
                    detections: None,
                    error: Some(e),
                    ..ScanResult::default()
                })
            }
            Err(e) => Err(e),
        }
    }
    fn run_id() -> String {
        static ID: AtomicUsize = AtomicUsize::new(0);
        format!(
            "{:016x}{:08x}{:08x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        )
    }
    fn profile_line(
        c: &Config,
        index: usize,
        total: usize,
        source: &Source,
        status: &str,
        d: &Detection,
        t: Timing,
    ) {
        if !c.profile {
            return;
        }
        println!("PROFILE [{index:02}/{total}] {status} target={} probe={:.3}s detect={:.3}s ocr_predict={:.3}s ocr_frame={:.3}s samples={} coarse={} refine={} gate_skipped={} ocr_requests={} ocr_successes={} ocr_cache_hits={} ocr_skipped={} termination={} trim={:.3}s total={:.3}s | {}",if d.target.is_empty(){"-"}else{&d.target},t.probe,d.detect_seconds,d.ocr_seconds,d.ocr_frame_seconds,d.sampled_count,d.ocr_coarse_frames,d.ocr_refine_frames,d.ocr_gate_skipped_frames,d.ocr_requests,d.ocr_successes,d.ocr_cache_hits,d.ocr_skipped,if d.termination_reason.is_empty(){"-"}else{&d.termination_reason},t.trim,t.total,source.path.file_name().unwrap().to_string_lossy());
    }
    fn skip(
        c: &Config,
        index: usize,
        total: usize,
        source: &Source,
        d: &Detection,
        t: Timing,
        early: bool,
    ) {
        let name = source.path.file_name().unwrap().to_string_lossy();
        if early {
            println!(
                "[{index:02}/{total}] SKIP {} {}s < {:.3}s {} lang={} | {name}",
                d.target,
                if d.event_secs.is_empty() {
                    format!("{:.3}", d.event_sec.unwrap_or(0.0))
                } else {
                    d.event_secs.clone()
                },
                c.min_event_sec.max(0.0),
                d.method,
                source.language.code()
            )
        } else {
            println!(
                "[{index:02}/{total}] SKIP {} {} lang={} | {name} | {}",
                d.target,
                d.method,
                source.language.code(),
                d.text.chars().take(80).collect::<String>()
            )
        }
        profile_line(c, index, total, source, "SKIP", d, t)
    }
    pub fn run(c: Config) -> Result<u8, String> {
        crate::cancellation::check()?;
        let started = Instant::now();
        let mut phases = PhaseTimings::default();
        let run_id = run_id();
        if c.ocr_width < 0 {
            return Err("OCR widths must be non-negative".into());
        }
        let inputs = source_files::select(&c)?;
        let paths = OutputPaths::plan(&c, &inputs).map_err(|e| e.to_string())?;
        let mut tools = FfmpegTools::discover(c.ffmpeg.as_deref(), c.ffprobe.as_deref())
            .map_err(|e| e.to_string())?;
        tools.verbose = c.verbose;
        let candidates_path = source_files::discover_candidates(&c, &inputs);
        let candidates = source_files::read_candidates(candidates_path.as_deref())?;
        let mut settings = EngineSettings::from_config(&c)?;
        let jobs = workers(&c, inputs.sources.len());
        let cpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        settings.threads = settings
            .threads
            .min((cpu / jobs.min(inputs.sources.len())).max(1));
        settings
            .identity
            .push_str(&format!(":effective-threads={}", settings.threads));
        let cache_dir = if c.no_fast_path {
            None
        } else {
            c.cache_dir.clone().or_else(|| {
                c.fast_path
                    .then(|| inputs.base_folder.join(".pubg_highlight_trim_cache"))
            })
        };
        let full_scan = match c.scan_mode {
            crate::config::ScanMode::Fast => false,
            crate::config::ScanMode::Full => true,
            crate::config::ScanMode::Auto => !candidates_path.as_ref().is_some_and(|p| p.exists()),
        };
        if c.verbose {
            println!("windows_only=true\nsources={}\ndetector=ocr\ngame_lang={}\ndetected_game_langs={}\ntarget={}\nmin_event_sec={:.3}\nmolotov_elim_before={:.3}\nffmpeg={}\noutput_dir={}\nmerge_output={}\nmerge={}\njobs={}{}\nfast_path={}\nbrightness_gate_mode={}\ncache_dir={}\nscan_mode={}\nfull_scan={}\ncandidate_csv={}",inputs.sources.len(),c.game_lang.code(),source_files::language_counts(&inputs).into_iter().map(|(k,v)|format!("{k}:{v}")).collect::<Vec<_>>().join(","),c.target.code(),c.min_event_sec.max(0.0),c.molotov_elim_before.max(0.0),tools.ffmpeg.display(),paths.directory.display(),paths.merged.display(),c.merge_enabled(inputs.single_file),jobs,if c.jobs=="auto"{" (auto)"}else{""},c.fast_path,match c.brightness_gate_mode.unwrap_or(crate::config::GateMode::Light){crate::config::GateMode::Full=>"full",crate::config::GateMode::Light=>"light",crate::config::GateMode::None=>"none"},cache_dir.as_ref().map(|p|p.display().to_string()).unwrap_or_default(),format!("{:?}",c.scan_mode).to_lowercase(),full_scan,candidates_path.as_ref().map(|p|p.display().to_string()).unwrap_or_default());
        }
        let ffmpeg_version = tools.version_token();
        let mut codec_counts = BTreeMap::<String, usize>::new();
        for source in &inputs.sources {
            *codec_counts
                .entry(
                    tools
                        .source_codec(&source.path)
                        .unwrap_or_else(|| "unknown".into()),
                )
                .or_default() += 1;
        }
        let total = inputs.sources.len();
        progress(0, total, jobs);
        phases.enter(Phase::Scan, started.elapsed().as_secs_f64());
        let mut cached_sources = vec![false; total];
        let mut source_scan_configs = Vec::new();
        let mut results: Vec<Option<ScanResult>> = vec![None; total];
        let mut queue = Vec::new();
        let mut cache_status = BTreeMap::new();
        let (mut cache_hits, mut cache_misses) = (0usize, 0usize);
        let (mut cache_read_seconds, mut cache_write_seconds) = (0.0, 0.0);
        let mut completed = 0;
        for (index, source) in inputs.sources.iter().enumerate() {
            let config = scan_adapter::source_config(&c, source, candidates_path.as_deref());
            config.validate()?;
            source_scan_configs.push(json!({"source_index":index + 1,"force_full_scan":source.force_full_scan,"config":config}));
            let mut times = candidates
                .get(
                    &source
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                )
                .cloned()
                .unwrap_or_default();
            times.sort_by(f64::total_cmp);
            let key = if cache_dir.is_some() {
                Some(
                    persistent_cache::cache_key(
                        &source.path,
                        &json!({"config":config,"candidate_times":times}),
                        &settings.identity,
                    )
                    .map_err(|e| e.to_string())?,
                )
            } else {
                None
            };
            let name = source
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if let (Some(dir), Some(key)) = (&cache_dir, &key) {
                let read_started = Instant::now();
                let cached = persistent_cache::read::<CachedScan>(dir, key);
                cache_read_seconds += read_started.elapsed().as_secs_f64();
                if let Some(mut cached) = cached {
                    if let Some(provider) = &mut cached.provider {
                        if let Some(object) = provider.as_object_mut() {
                            object.remove("source");
                        }
                        provider["source_index"] = json!(index + 1);
                        provider["provider_mode"] = json!(c.ocr_provider.code());
                        if let Some(reason) = provider["fallback_reason"].as_str() {
                            provider["fallback_reason"] = json!(
                                crate::provider_diagnostics::redact(reason, &settings.model_dir)
                            );
                        }
                        provider["cache_hit"] = json!(true);
                    }
                    let detections = cached.detections;
                    let probe = Instant::now();
                    let duration = if let Some(d) = detections.first() {
                        Ok(d.duration_sec)
                    } else {
                        tools.duration_sec(&source.path)
                    };
                    let probe_seconds = if detections.is_empty() {
                        probe.elapsed().as_secs_f64()
                    } else {
                        0.0
                    };
                    results[index] = Some(match duration {
                        Ok(duration) => ScanResult {
                            provider: cached.provider,
                            duration,
                            source_seconds: 0.0,
                            probe_seconds,
                            detections: Some(detections),
                            error: None,
                            ..ScanResult::default()
                        },
                        Err(e) if is_unreadable(&e) => ScanResult {
                            provider: cached.provider,
                            duration: 0.0,
                            source_seconds: 0.0,
                            probe_seconds,
                            detections: None,
                            error: Some(format!(
                                "ffprobe duration probe failed for {}: {e}",
                                source.path.display()
                            )),
                            ..ScanResult::default()
                        },
                        Err(e) => return Err(e.to_string()),
                    });
                    cache_status.insert(name, "hit".to_string());
                    cache_hits += 1;
                    cached_sources[index] = true;
                    completed += 1;
                    progress(completed, total, jobs);
                    continue;
                }
                cache_misses += 1;
            }
            cache_status.insert(
                name,
                if cache_dir.is_some() {
                    "miss"
                } else {
                    "disabled"
                }
                .into(),
            );
            queue.push((
                index,
                ScanJob {
                    source: source.clone(),
                    config,
                    candidates: times,
                    cache_key: key,
                },
            ));
        }
        let next = AtomicUsize::new(0);
        let (tx, rx) = mpsc::channel();
        // No unbounded future queue or shared mutable OCR session. At most min(jobs,
        // source_count) workers each own one reusable session per OCR language.
        let scan_error = std::thread::scope(|scope| {
            for _ in 0..jobs.min(queue.len()) {
                let (tx, queue, next, settings, tools, cache_dir) =
                    (tx.clone(), &queue, &next, &settings, &tools, &cache_dir);
                let profile = c.profile;
                let verbose = c.verbose;
                let mode = c.ocr_provider;
                scope.spawn(move || {
                    let mut engines = HashMap::new();
                    loop {
                        if crate::cancellation::is_cancelled() {
                            break;
                        }
                        let slot = next.fetch_add(1, Ordering::Relaxed);
                        let Some((index, job)) = queue.get(slot) else {
                            break;
                        };
                        let source_started = Instant::now();
                        let mut provider = None;
                        let result: Result<ScanResult, String> = (|| {
                            let lang = if job.source.language.code() == "en" {
                                "en"
                            } else {
                                "ch"
                            };
                            if !engines.contains_key(lang) {
                                let init = Instant::now();
                                let engine = settings.engine()?;
                                engines.insert(lang, engine);
                                if profile {
                                    println!(
                                        "PROFILE ocr_init={:.3}s lang={lang}",
                                        init.elapsed().as_secs_f64()
                                    );
                                }
                            }
                            let scan_result=scan(job, tools, &engines[lang]);
                            let info=engines[lang].provider_info();
                            provider=Some(json!({"source_index":index + 1,"provider_mode":mode.code(),"provider_requested":crate::provider_name(info.requested),"provider_actual":crate::provider_name(info.provider),"device_id":info.device_id,"model_version":settings.manifest.model_version,"fallback_reason":info.fallback_reason.as_deref().map(|reason| crate::provider_diagnostics::redact(reason, &settings.model_dir)),"cache_hit":false}));
                            let mut result=scan_result?;
                            result.provider=provider.clone();
                            Ok(result)
                        })();
                        let mut write_seconds = 0.0;
                        let result = result.and_then(|mut r| {
                            r.source_seconds = source_started.elapsed().as_secs_f64();
                            if let (Some(dir), Some(key), Some(d)) =
                                (cache_dir, &job.cache_key, &r.detections)
                            {
                                let t = Instant::now();
                                persistent_cache::write(dir, key, &CachedScan{detections:d.clone(),provider:r.provider.clone()}).map_err(|e| e.to_string())?;
                                write_seconds = t.elapsed().as_secs_f64();
                            }
                            Ok(r)
                        });
                        if verbose {
                            if let Err(error) = &result {
                                eprintln!(
                                    "OCR mode={} requested={} actual={} device_id={} fallback={} cache_hit=false",
                                    mode.code(),
                                    crate::provider_name(settings.provider),
                                    provider.as_ref().and_then(|p| p["provider_actual"].as_str()).unwrap_or("unknown"),
                                    settings.device_id,
                                    provider.as_ref().map(|p| p["fallback_reason"].as_str().unwrap_or("").to_owned()).unwrap_or_else(|| crate::provider_diagnostics::redact(error, &settings.model_dir)),
                                );
                            }
                        }
                        if tx.send((*index, result, write_seconds)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let mut failure = None;
            for (index, result, write) in rx {
                cache_write_seconds += write;
                match result {
                    Ok(result) => results[index] = Some(result),
                    Err(e) => {
                        if failure.is_none() {
                            failure = Some(e)
                        }
                    }
                }
                completed += 1;
                progress(completed, total, jobs);
            }
            failure
        });
        let active_ocr_workers = jobs.min(queue.len());
        phases.enter(Phase::Planning, started.elapsed().as_secs_f64());
        let mut telemetry_current = reports::ScanTelemetry::default();
        let mut telemetry_historical = HistoricalSnapshotTelemetry::default();
        for (index, result) in results.iter().enumerate() {
            if let Some(result) = result {
                telemetry_current.probe_seconds_sum += result.probe_seconds;
                if cached_sources[index] {
                    telemetry_historical.add(result);
                } else if let (Some(stats), Some(work)) =
                    (&result.current_stats, &result.current_work)
                {
                    telemetry_current.add_current(stats, work);
                } else {
                    telemetry_current.sources += 1;
                    telemetry_current.sources_without_detection_telemetry += 1;
                }
            }
        }
        let providers = results
            .iter()
            .filter_map(|r| r.as_ref().and_then(|r| r.provider.clone()))
            .collect::<Vec<_>>();
        if c.verbose {
            for provider in &providers {
                eprintln!(
                    "OCR mode={} requested={} actual={} device_id={} fallback={} cache_hit={}",
                    c.ocr_provider.code(),
                    provider["provider_requested"].as_str().unwrap_or("unknown"),
                    provider["provider_actual"].as_str().unwrap_or("unknown"),
                    c.ocr_device_id,
                    provider["fallback_reason"].as_str().unwrap_or(""),
                    provider["cache_hit"].as_bool().unwrap_or(false),
                );
            }
        }
        if let Some(error) = scan_error {
            return Err(error);
        }
        crate::cancellation::check()?;
        // Never destroy old output merely because a model or scanning backend failed.
        paths.prepare(&c, &inputs).map_err(|e| e.to_string())?;
        let mut records = Vec::<Record>::new();
        let mut clips = Vec::new();
        let mut targets: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        let mut archived = Vec::new();
        let mut encoders = BTreeMap::<String, usize>::new();
        let input_files = inputs
            .sources
            .iter()
            .map(|s| s.path.clone())
            .collect::<Vec<_>>();
        // Prepare every source before emitting any trim: a later recording may
        // contain more of the requested context. Keep per-source planning order.
        let configs = inputs
            .sources
            .iter()
            .map(|source| PlanningConfig {
                seconds_before: c.before,
                seconds_after: c.after,
                min_event_sec: Some(c.min_event_sec),
                molotov_elim_before: Some(c.molotov_elim_before),
                game_lang: source.language.code().into(),
                split_targets: c.split_targets(),
            })
            .collect::<Vec<_>>();
        let mut early_skips = Vec::new();
        let mut prepared = Vec::new();
        for (index, source) in inputs.sources.iter().enumerate() {
            let result = results[index]
                .as_mut()
                .ok_or("worker did not return a scan result")?;
            let (kept, skipped) = match result.detections.as_ref() {
                Some(d) if !(d.len() == 1 && d[0].event_sec.is_none()) => {
                    planning::prepare_detections(d.clone(), &configs[index])
                }
                _ => (Vec::new(), Vec::new()),
            };
            prepared.push((source.path.clone(), kept));
            early_skips.push(skipped);
        }
        phases.enter(Phase::Dedupe, started.elapsed().as_secs_f64());
        let dedupe_candidates: usize = prepared
            .iter()
            .flat_map(|(_, ds)| ds.iter())
            .filter(|d| d.event_sec.is_some() && !d.event_key.is_empty())
            .count();
        let dedupe_workers = if dedupe_candidates > 0 { 1 } else { 0 };
        progress_phase_start("dedupe", dedupe_candidates, dedupe_workers);
        let (duplicates, dedupe_work) = crate::dedupe::cross_source_duplicates_profiled(
            &prepared,
            &mut |path| {
                crate::media::FfmpegFrameSource::open(path, &tools, None)
                    .map(|source| Box::new(source) as Box<dyn crate::dedupe::VerificationFrames>)
                    .map_err(|e| e.to_string())
            },
            Some(&|current: usize, total: usize| {
                // Guard: do not emit false done after cancellation.
                if !crate::cancellation::is_cancelled() {
                    progress_phase_current("dedupe", current, total, dedupe_workers);
                }
            }),
        );
        progress_phase_done("dedupe", dedupe_candidates, dedupe_workers)?;
        phases.enter(Phase::Planning, started.elapsed().as_secs_f64());
        // Final grouping, sequence numbers and destinations are fixed for the
        // whole run before any encoding. Keep source and per-source plan order.
        let mut duplicate_skips = vec![Vec::new(); total];
        let mut planned = Vec::new();
        let mut sequence = 0;
        for (source_index, source) in inputs.sources.iter().enumerate() {
            crate::cancellation::check()?;
            let mut retained = Vec::new();
            for (position, mut d) in std::mem::take(&mut prepared[source_index].1)
                .into_iter()
                .enumerate()
            {
                if duplicates.contains(&(source_index, position)) {
                    d.method = "skipped-cross-source-duplicate".into();
                    duplicate_skips[source_index].push(d);
                } else {
                    retained.push(d);
                }
            }
            let mut tasks = Vec::new();
            for clip in planning::plan_prepared(retained, &configs[source_index]) {
                sequence += 1;
                let d = &clip.detection;
                let output = if c.dry_run {
                    None
                } else {
                    let suffix = if matches!(d.event_count.as_str(), "" | "0" | "1") {
                        String::new()
                    } else {
                        format!("x{}", d.event_count)
                    };
                    let target = if d.target.is_empty() {
                        "event"
                    } else {
                        &d.target
                    };
                    let name = source.path.file_name().unwrap().to_string_lossy();
                    Some(
                        crate::output_files::unique_path(
                            &paths
                                .directory
                                .join("clips")
                                .join(format!("{sequence:03}_{target}{suffix}_{name}")),
                        )
                        .map_err(|e| e.to_string())?,
                    )
                };
                tasks.push(PlannedTrim {
                    sequence,
                    clip,
                    output,
                });
            }
            planned.push(tasks);
        }
        phases.enter(Phase::Trim, started.elapsed().as_secs_f64());
        let trim_total_clips: usize = planned.iter().map(|tasks| tasks.len()).sum();
        let trim_workers = if trim_total_clips > 0 && !c.dry_run {
            if c.trim_jobs == "2" {
                2.min(trim_total_clips)
            } else {
                1
            }
        } else {
            0
        };
        if c.dry_run {
            progress_phase_skipped("trim", "dry-run", trim_total_clips);
        } else {
            progress_phase_start("trim", trim_total_clips, trim_workers);
        }
        let mut trim_completed_clips: usize = 0;
        let mut trim_encoder = crate::media::TrimEncoder::new(&tools);
        if trim_workers > 0 {
            trim_encoder
                .ensure_capability()
                .map_err(|e| e.to_string())?;
        }
        let trim_jobs = planned
            .iter()
            .zip(&inputs.sources)
            .flat_map(|(tasks, source)| {
                tasks.iter().filter_map(move |task| {
                    task.output.as_ref().map(|output| {
                        (
                            source.path.clone(),
                            output.clone(),
                            task.clip.start_sec,
                            task.clip.end_sec - task.clip.start_sec,
                        )
                    })
                })
            });
        let mut trim_queue = TrimQueue {
            jobs: trim_jobs,
            pending: Default::default(),
            encoder: trim_encoder.clone(),
            tools: tools.clone(),
            inputs: std::sync::Arc::new(input_files.clone()),
            stop: Default::default(),
            failure: Default::default(),
        };
        for _ in 0..trim_workers {
            trim_queue.submit()?;
        }
        let mut trim_tasks = Vec::new();
        let (mut encode_sum, mut output_probe_sum, mut publication_sum, mut task_total_sum) =
            (0.0, 0.0, 0.0, 0.0);
        // Workers may finish out of order; publication and all downstream
        // consumers follow the already fixed planning sequence.
        for (index, source) in inputs.sources.iter().enumerate() {
            crate::cancellation::check()?;
            let index = index + 1;
            let result = results[index - 1]
                .take()
                .ok_or("worker did not return a scan result")?;
            let source_started = Instant::now();
            let scan_cost = if jobs == 1 {
                result.source_seconds
            } else {
                0.0
            };
            let name = source.path.file_name().unwrap().to_string_lossy();
            if source.force_full_scan && !full_scan {
                println!("[{index:02}/{total}] full_scan=true multi-kill-source | {name}")
            }
            let Some(detections) = result.detections else {
                let reason = result
                    .error
                    .unwrap_or_default()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "[{index:02}/{total}] SKIP {} video-unreadable lang={} | {name}{}",
                    c.target.code(),
                    source.language.code(),
                    if c.verbose && !reason.is_empty() {
                        format!(" | {reason}")
                    } else {
                        String::new()
                    }
                );
                let mut row = reports::blank_record(
                    index,
                    &source.path,
                    result.duration,
                    "unreadable",
                    "video-unreadable",
                    "ocr",
                );
                row.insert(
                    "ProbeSeconds".into(),
                    format!("{:.3}", result.probe_seconds),
                );
                row.insert(
                    "TotalSeconds".into(),
                    format!("{:.3}", scan_cost + source_started.elapsed().as_secs_f64()),
                );
                records.push(row);
                continue;
            };
            if detections.len() == 1 && detections[0].event_sec.is_none() {
                let d = &detections[0];
                let timing = Timing {
                    probe: result.probe_seconds,
                    trim: 0.0,
                    total: scan_cost + source_started.elapsed().as_secs_f64(),
                };
                skip(&c, index, total, source, d, timing, false);
                records.push(reports::skipped_record(index, &source.path, d, timing));
                continue;
            }
            for d in std::mem::take(&mut early_skips[index - 1]) {
                let timing = Timing {
                    probe: result.probe_seconds,
                    trim: 0.0,
                    total: scan_cost + source_started.elapsed().as_secs_f64(),
                };
                skip(&c, index, total, source, &d, timing, true);
                records.push(reports::skipped_record(index, &source.path, &d, timing));
            }
            for d in std::mem::take(&mut duplicate_skips[index - 1]) {
                let timing = Timing {
                    probe: result.probe_seconds,
                    trim: 0.0,
                    total: scan_cost + source_started.elapsed().as_secs_f64(),
                };
                skip(&c, index, total, source, &d, timing, false);
                records.push(reports::skipped_record(index, &source.path, &d, timing));
            }
            let mut produced = false;
            for task in &planned[index - 1] {
                crate::cancellation::check()?;
                let plan = &task.clip;
                let d = &plan.detection;
                let mut encoder = String::new();
                let mut output = String::new();
                let mut trim_seconds = 0.0;
                if let Some(path) = &task.output {
                    debug_assert_eq!(task.sequence, clips.len() + 1);
                    let encoded = trim_queue.next()?;
                    crate::cancellation::check()?;
                    encoder = encoded.encoder;
                    trim_encoder.record_parallel_codec(&encoder);
                    trim_seconds = encoded.encode_seconds;
                    let output_probe_seconds = encoded.probe_seconds;
                    let task_started = encoded.started;
                    let staged = encoded.staged;
                    let publication_started = Instant::now();
                    staged
                        .publish(&input_files, false)
                        .map_err(|e| e.to_string())?;
                    let publication_seconds = publication_started.elapsed().as_secs_f64();
                    let total_seconds = task_started.elapsed().as_secs_f64();
                    encode_sum += trim_seconds;
                    output_probe_sum += output_probe_seconds;
                    publication_sum += publication_seconds;
                    task_total_sum += total_seconds;
                    trim_tasks.push(json!({"sequence":task.sequence,
                        "encode_seconds":reports::round3(trim_seconds),
                        "output_probe_seconds":reports::round3(output_probe_seconds),
                        "publication_seconds":reports::round3(publication_seconds),
                        "total_seconds":reports::round3(total_seconds)}));
                    *encoders.entry(encoder.clone()).or_default() += 1;
                    clips.push(path.clone());
                    if c.split_targets() {
                        targets
                            .entry(d.target.clone())
                            .or_default()
                            .push(path.clone())
                    }
                    output = path.display().to_string();
                    produced = true;
                    // Only count clips that were successfully published (not dry-run).
                    trim_completed_clips += 1;
                    progress_phase_current(
                        "trim",
                        trim_completed_clips,
                        trim_total_clips,
                        trim_workers,
                    );
                    trim_queue.submit()?;
                }
                let t = Timing {
                    probe: result.probe_seconds,
                    trim: trim_seconds,
                    total: scan_cost + source_started.elapsed().as_secs_f64(),
                };
                println!(
                    "[{index:02}/{total}] INCLUDE {} {}s {:.3}-{:.3} {} lang={} | {name}",
                    d.target,
                    if d.event_secs.is_empty() {
                        format!("{:.3}", d.event_sec.unwrap_or(0.0))
                    } else {
                        d.event_secs.clone()
                    },
                    plan.start_sec,
                    plan.end_sec,
                    d.method,
                    source.language.code()
                );
                profile_line(&c, index, total, source, "INCLUDE", d, t);
                records.push(reports::included_record(
                    index,
                    &source.path,
                    d,
                    [plan.start_sec, plan.end_sec],
                    &encoder,
                    &output,
                    t,
                ));
            }
            if produced {
                archived.push(source.path.clone())
            }
        }
        if !c.dry_run {
            progress_phase_done("trim", trim_completed_clips, trim_workers)?;
        }
        // Keep the historical CSV failure gate before merge/archive, but charge
        // this reporting interval only to reporting, not to trim or merge.
        phases.enter(Phase::Reporting, started.elapsed().as_secs_f64());
        reports::write_records(&paths.directory, &run_id, &records, &cache_status)
            .map_err(|e| e.to_string())?;
        phases.enter(Phase::Merge, started.elapsed().as_secs_f64());
        let mut merge_outputs = Vec::new();
        let (mut merge_duration, mut merge_size) = (0.0, 0.0);
        if c.merge_enabled(inputs.single_file) {
            // Build the actual output list only when there are clips and not a dry-run.
            let outputs: Vec<_> = if !clips.is_empty() && !c.dry_run {
                if c.split_targets() {
                    ["self-death", "own-kill"]
                        .into_iter()
                        .filter_map(|target| {
                            targets.get(target).map(|clips| {
                                (
                                    target,
                                    clips,
                                    paths
                                        .split_merges
                                        .iter()
                                        .find(|(t, _)| t == target)
                                        .unwrap()
                                        .1
                                        .clone(),
                                )
                            })
                        })
                        .collect()
                } else {
                    vec![(c.target.code(), &clips, paths.merged.clone())]
                }
            } else {
                vec![]
            };
            let merge_total = outputs.len();
            let merge_workers = if merge_total > 0 { 1 } else { 0 };
            if merge_total == 0 {
                progress_phase_skipped("merge", if c.dry_run { "dry-run" } else { "no-clips" }, 0);
            } else {
                progress_phase_start("merge", merge_total, merge_workers);
            }
            let mut merge_done = 0;
            for (target, clips, destination) in outputs {
                crate::cancellation::check()?;
                let staged = crate::media_output::MediaOutput::reserve(
                    &destination,
                    c.overwrite,
                    &input_files,
                )
                .map_err(|e| e.to_string())?;
                tools
                    .concat_clips(clips, staged.temporary())
                    .map_err(|e| e.to_string())?;
                let duration = tools
                    .duration_sec(staged.temporary())
                    .map_err(|e| e.to_string())?;
                let size = fs::metadata(staged.temporary())
                    .map_err(|e| e.to_string())?
                    .len() as f64
                    / 1048576.0;
                let list = staged
                    .publish(&input_files, true)
                    .map_err(|e| e.to_string())?
                    .ok_or("merge concat list was not published")?;
                merge_duration += duration;
                merge_size += size;
                merge_outputs.push(json!({"target":target,"path":destination,"concat_list":list}));
                merge_done += 1;
                progress_phase_current("merge", merge_done, merge_total, merge_workers);
            }
            if merge_total > 0 {
                progress_phase_done("merge", merge_done, merge_workers)?;
            }
        } else {
            progress_phase_skipped("merge", "disabled", 0);
        }
        phases.enter(Phase::Archive, started.elapsed().as_secs_f64());
        let (mut originals_dir, mut originals_moved, mut originals_failed) = (String::new(), 0, 0);
        if c.move_originals && !c.dry_run {
            crate::cancellation::check()?;
            let (dir, moved, failed) =
                crate::output_files::archive_sources(&archived, &paths.directory)
                    .map_err(|e| e.to_string())?;
            originals_dir = dir.display().to_string();
            originals_moved = moved;
            originals_failed = failed;
        }
        phases.enter(Phase::Reporting, started.elapsed().as_secs_f64());
        let included = records.iter().filter(|r| r["Status"] == "included").count();
        let skipped = records
            .iter()
            .filter(|r| r["Status"] == "skipped" || r["Status"] == "unreadable")
            .count();
        let unreadable = records
            .iter()
            .filter(|r| r["Status"] == "unreadable")
            .count();
        // Reporting is frozen below after ordinary report preparation.
        let candidate_output = paths.directory.join("candidate_events.csv");
        let profile_path = paths.directory.join("profile.json");
        let csv_path = paths.directory.join("检测与裁剪记录.csv");
        let mut termination = reports::counts(&records, "TerminationReason");
        termination.remove("");
        let mut profile = json!({
            "schema_version":"2", "run_id":run_id,
            "timing_boundary":"before_final_timing_serialization_and_emission",
            "aggregates":{"cache_read_seconds_sum":reports::round3(cache_read_seconds),"cache_write_seconds_sum":reports::round3(cache_write_seconds),"trim_encode_seconds_sum":reports::round3(encode_sum),"trim_output_probe_seconds_sum":reports::round3(output_probe_sum),"trim_publication_seconds_sum":reports::round3(publication_sum),"trim_task_total_seconds_sum":reports::round3(task_total_sum)},
            "telemetry":{"current":telemetry_current,"cached_historical":telemetry_historical,"current_basis":"completed_scan_report","cached_historical_basis":"maximum_detection_snapshot_through_last_accepted_event_not_completed_scan"},
            "trim_tasks":trim_tasks,
            "dedupe_work":dedupe_work,
            "counts":{"sources":total,"candidates":records.iter().filter(|r|!r["EventSec"].is_empty()).count(),"outputs":clips.len()},
            "runtime":{"ocr_worker_capacity":jobs.min(total),"ocr_workers_active":active_ocr_workers,"trim_workers":trim_workers,"trim_jobs_requested":c.trim_jobs,"trim_encoder":trim_encoder.diagnostics(),"trim_validation":"ffprobe_duration_strict","ocr_provider_mode":c.ocr_provider.code(),"ocr_threads_per_worker":settings.threads,"source_codec_counts":codec_counts,"ffmpeg_version":ffmpeg_version,"scan_mode":format!("{:?}",c.scan_mode).to_lowercase(),"full_scan":full_scan,"candidate_input_present":candidates_path.is_some(),"brightness_gate_mode":format!("{:?}",c.brightness_gate_mode.unwrap_or(crate::config::GateMode::Light)).to_lowercase(),"validation":"strict","dry_run":c.dry_run,"source_scan_configs":source_scan_configs},
            "cache":{"enabled":cache_dir.is_some(),"directory":cache_dir.as_ref().map(|p|p.display().to_string()).unwrap_or_default(),"hits":cache_hits,"misses":cache_misses,"hit_rate":if cache_hits+cache_misses==0{0.0}else{reports::round3(cache_hits as f64/(cache_hits+cache_misses)as f64)}},
            "candidate_csv":candidate_output,"termination_reasons":termination,"ocr_providers":providers,
            "ocr_providers_current":providers.iter().filter(|p|p["cache_hit"] != true).collect::<Vec<_>>(),
            "ocr_providers_cached_historical":providers.iter().filter(|p|p["cache_hit"] == true).collect::<Vec<_>>()
        });
        let summary = json!({"source_count":total,"included_count":included,"skipped_count":skipped,"unreadable_count":unreadable,"dry_run":c.dry_run,"detector":"ocr","game_lang":c.game_lang.code(),"detected_game_langs":source_files::language_counts(&inputs),"min_event_sec":c.min_event_sec.max(0.0),"molotov_elim_before":c.molotov_elim_before.max(0.0),"ocr_unavailable_reason":"","output_dir":paths.directory,"merge":c.merge_enabled(inputs.single_file),"split_merge_by_target":c.split_targets(),"merge_outputs":merge_outputs,"merge_output":merge_outputs.first().and_then(|m|m.get("path")).cloned().unwrap_or(json!("")),"scan_mode":format!("{:?}",c.scan_mode).to_lowercase(),"full_scan":full_scan,"candidate_csv":candidates_path.as_ref().map(|p|p.display().to_string()).unwrap_or_default(),"candidate_events_csv":candidate_output,"profile":profile_path,"run_id":run_id,"cache_enabled":cache_dir.is_some(),"cache_hits":cache_hits,"cache_misses":cache_misses,"concat_list":merge_outputs.first().and_then(|m|m.get("concat_list")).cloned().unwrap_or(json!("")),"originals_dir":originals_dir,"originals_moved":originals_moved,"originals_failed":originals_failed,"csv":csv_path,"merge_duration_sec":reports::round3(merge_duration),"merge_size_mb":(merge_size*10.0).round_ties_even()/10.0,"methods":reports::counts(&records,"Method"),"detectors":reports::counts(&records,"Detector"),"encoders":encoders,"profile_total_sec":0.0});
        let mut summary = summary;
        summary["ocr_providers"] = json!(providers);
        // Freeze once, after report preparation and CSV writes. Final timing
        // JSON serialization, writes and stdout emission are excluded by contract.
        profile["stages"] = phases.finish(started.elapsed().as_secs_f64());
        summary["profile_total_sec"] = profile["stages"]["total"].clone();
        reports::write_json(&profile_path, &profile).map_err(|e| e.to_string())?;
        if c.profile {
            println!(
                "PROFILE {}",
                json!({"run_id":run_id,"stages":profile["stages"]})
            );
        }
        let summary_text = reports::encode_summary(
            &summary,
            c.min_event_sec.max(0.0),
            c.molotov_elim_before.max(0.0),
        )
        .map_err(|e| e.to_string())?;
        fs::write(paths.directory.join("summary.json"), &summary_text)
            .map_err(|e| e.to_string())?;
        println!("SUMMARY {summary_text}");
        Ok(if included == 0 { 2 } else { 0 })
    }
}

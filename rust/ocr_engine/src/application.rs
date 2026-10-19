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
        reports::{self, Record, Timing},
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
    #[derive(Clone, Serialize, Deserialize)]
    struct ScanResult {
        #[serde(default)]
        provider: Option<Value>,
        duration: f64,
        probe_seconds: f64,
        source_seconds: f64,
        detections: Option<Vec<Detection>>,
        error: Option<String>,
    }
    #[derive(Serialize, Deserialize)]
    struct CachedScan {
        detections: Vec<Detection>,
        provider: Option<Value>,
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
        let setup_seconds = started.elapsed().as_secs_f64();
        if c.profile {
            println!("profile=true setup={setup_seconds:.3}s")
        }
        let total = inputs.sources.len();
        progress(0, total, jobs);
        let scan_started = Instant::now();
        let mut results: Vec<Option<ScanResult>> = vec![None; total];
        let mut queue = Vec::new();
        let mut cache_status = BTreeMap::new();
        let (mut cache_hits, mut cache_misses) = (0usize, 0usize);
        let (mut cache_read_seconds, mut cache_write_seconds) = (0.0, 0.0);
        let mut completed = 0;
        for (index, source) in inputs.sources.iter().enumerate() {
            let config = scan_adapter::source_config(&c, source, candidates_path.as_deref());
            config.validate()?;
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
                        },
                        Err(e) => return Err(e.to_string()),
                    });
                    cache_status.insert(name, "hit".to_string());
                    cache_hits += 1;
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
        let duplicates = crate::dedupe::cross_source_duplicates(&prepared, &mut |path| {
            crate::media::FfmpegFrameSource::open(path, &tools, None)
                .map(|source| Box::new(source) as Box<dyn crate::dedupe::VerificationFrames>)
                .map_err(|e| e.to_string())
        });
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
            let mut retained = Vec::new();
            for (position, mut d) in std::mem::take(&mut prepared[index - 1].1)
                .into_iter()
                .enumerate()
            {
                if duplicates.contains(&(index - 1, position)) {
                    d.method = "skipped-cross-source-duplicate".into();
                    let timing = Timing {
                        probe: result.probe_seconds,
                        trim: 0.0,
                        total: scan_cost + source_started.elapsed().as_secs_f64(),
                    };
                    skip(&c, index, total, source, &d, timing, false);
                    records.push(reports::skipped_record(index, &source.path, &d, timing));
                } else {
                    retained.push(d);
                }
            }
            let mut produced = false;
            for plan in planning::plan_prepared(retained, &configs[index - 1]) {
                crate::cancellation::check()?;
                let d = &plan.detection;
                let mut encoder = String::new();
                let mut output = String::new();
                let mut trim_seconds = 0.0;
                if !c.dry_run {
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
                    let path = crate::output_files::unique_path(
                        &paths
                            .directory
                            .join("clips")
                            .join(format!("{:03}_{target}{suffix}_{name}", clips.len() + 1)),
                    )
                    .map_err(|e| e.to_string())?;
                    let staged =
                        crate::media_output::MediaOutput::reserve(&path, false, &input_files)
                            .map_err(|e| e.to_string())?;
                    let trim_started = Instant::now();
                    encoder = tools
                        .trim_clip(
                            &source.path,
                            staged.temporary(),
                            plan.start_sec,
                            plan.end_sec - plan.start_sec,
                        )
                        .map_err(|e| e.to_string())?
                        .into();
                    trim_seconds = trim_started.elapsed().as_secs_f64();
                    tools
                        .duration_sec(staged.temporary())
                        .map_err(|e| e.to_string())?;
                    staged
                        .publish(&input_files, false)
                        .map_err(|e| e.to_string())?;
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
        let scan_seconds = scan_started.elapsed().as_secs_f64();
        reports::write_records(&paths.directory, &run_id, &records, &cache_status)
            .map_err(|e| e.to_string())?;
        let mut merge_outputs = Vec::new();
        let (mut merge_duration, mut merge_size) = (0.0, 0.0);
        if !clips.is_empty() && c.merge_enabled(inputs.single_file) && !c.dry_run {
            let merge_started = Instant::now();
            let outputs = if c.split_targets() {
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
                    .collect::<Vec<_>>()
            } else {
                vec![(c.target.code(), &clips, paths.merged.clone())]
            };
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
            }
            if c.profile {
                println!(
                    "PROFILE merge={:.3}s clips={}",
                    merge_started.elapsed().as_secs_f64(),
                    clips.len()
                )
            }
        }
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
        let included = records.iter().filter(|r| r["Status"] == "included").count();
        let skipped = records
            .iter()
            .filter(|r| r["Status"] == "skipped" || r["Status"] == "unreadable")
            .count();
        let unreadable = records
            .iter()
            .filter(|r| r["Status"] == "unreadable")
            .count();
        let total_seconds = started.elapsed().as_secs_f64();
        let candidate_output = paths.directory.join("candidate_events.csv");
        let profile_path = paths.directory.join("profile.json");
        let csv_path = paths.directory.join("检测与裁剪记录.csv");
        let mut termination = reports::counts(&records, "TerminationReason");
        termination.remove("");
        let trim_total = records
            .iter()
            .map(|r| r["TrimSeconds"].parse::<f64>().unwrap_or(0.0))
            .sum::<f64>();
        let profile = json!({"schema_version":"1","run_id":run_id,"stages":{"setup":reports::round3(setup_seconds),"scan":reports::round3(scan_seconds),"decode_probe":reports::round3(reports::source_total(&records,"ProbeSeconds")),"roi_gate":reports::round3(reports::source_total(&records,"OcrFrameSeconds")),"sampling":reports::round3(scan_seconds),"candidate_generation":reports::round3(scan_seconds),"ocr":reports::round3(reports::source_total(&records,"OcrSeconds")),"refine":reports::round3(reports::source_total(&records,"RefineOcrSeconds")),"assist":reports::round3(reports::source_total(&records,"AssistOcrSeconds")),"cache_read":reports::round3(cache_read_seconds),"cache_write":reports::round3(cache_write_seconds),"trim":reports::round3(trim_total),"merge":reports::round3(merge_duration),"total":reports::round3(total_seconds)},"counts":{"sources":total,"candidates":records.iter().filter(|r|!r["EventSec"].is_empty()).count(),"outputs":clips.len(),"ocr_requests":reports::source_total(&records,"OcrRequests")as u64,"ocr_successes":reports::source_total(&records,"OcrSuccesses")as u64,"ocr_skipped":reports::source_total(&records,"OcrSkipped")as u64,"ocr_cache_hits":reports::source_total(&records,"OcrCacheHits")as u64,"refine_calls":reports::source_total(&records,"OcrRefineFrames")as u64,"assist_calls":reports::source_total(&records,"AssistBudgetUsed")as u64},"cache":{"enabled":cache_dir.is_some(),"directory":cache_dir.as_ref().map(|p|p.display().to_string()).unwrap_or_default(),"hits":cache_hits,"misses":cache_misses,"hit_rate":if cache_hits+cache_misses==0{0.0}else{reports::round3(cache_hits as f64/(cache_hits+cache_misses)as f64)}},"candidate_csv":candidate_output,"termination_reasons":termination});
        let mut profile = profile;
        profile["ocr_providers"] = json!(providers);
        reports::write_json(&profile_path, &profile).map_err(|e| e.to_string())?;
        if c.profile {
            println!("PROFILE run_id={run_id} scan={scan_seconds:.3}s outputs={} cache_hits={cache_hits}",clips.len())
        }
        let summary = json!({"source_count":total,"included_count":included,"skipped_count":skipped,"unreadable_count":unreadable,"dry_run":c.dry_run,"detector":"ocr","game_lang":c.game_lang.code(),"detected_game_langs":source_files::language_counts(&inputs),"min_event_sec":c.min_event_sec.max(0.0),"molotov_elim_before":c.molotov_elim_before.max(0.0),"ocr_unavailable_reason":"","output_dir":paths.directory,"merge":c.merge_enabled(inputs.single_file),"split_merge_by_target":c.split_targets(),"merge_outputs":merge_outputs,"merge_output":merge_outputs.first().and_then(|m|m.get("path")).cloned().unwrap_or(json!("")),"scan_mode":format!("{:?}",c.scan_mode).to_lowercase(),"full_scan":full_scan,"candidate_csv":candidates_path.as_ref().map(|p|p.display().to_string()).unwrap_or_default(),"candidate_events_csv":candidate_output,"profile":profile_path,"run_id":run_id,"cache_enabled":cache_dir.is_some(),"cache_hits":cache_hits,"cache_misses":cache_misses,"concat_list":merge_outputs.first().and_then(|m|m.get("concat_list")).cloned().unwrap_or(json!("")),"originals_dir":originals_dir,"originals_moved":originals_moved,"originals_failed":originals_failed,"csv":csv_path,"merge_duration_sec":reports::round3(merge_duration),"merge_size_mb":(merge_size*10.0).round_ties_even()/10.0,"methods":reports::counts(&records,"Method"),"detectors":reports::counts(&records,"Detector"),"encoders":encoders,"profile_total_sec":reports::round3(total_seconds)});
        let mut summary = summary;
        summary["ocr_providers"] = json!(providers);
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

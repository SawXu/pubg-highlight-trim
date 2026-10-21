use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Phase {
    pub current: u64,
    pub total: u64,
    pub workers: Option<u64>,
    pub event: Option<String>,
    pub reason: Option<String>,
    pub planned: Option<u64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    pub run_id: u64,
    pub running: bool,
    pub cancelling: bool,
    pub exit_code: Option<i32>,
    pub outcome: Option<String>,
    pub summary: Option<Value>,
    pub phases: BTreeMap<String, Phase>,
    pub active_phase: Option<String>,
    pub included: u64,
    pub skipped: u64,
    pub error: Option<String>,
    #[serde(skip)]
    pub(crate) structured: bool,
}
impl RunState {
    pub fn line(&mut self, line: &str, stderr: bool) {
        if stderr {
            return;
        }
        if let Some(s) = line.strip_prefix("PROGRESS ") {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                if let Some(name) = v["phase"].as_str() {
                    self.structured = true;
                    if ["scan", "dedupe", "trim", "merge"].contains(&name) {
                        let previous = self.phases.get(name).cloned().unwrap_or_default();
                        let current = v["current"].as_u64().unwrap_or(previous.current);
                        let total = v["total"].as_u64().unwrap_or(previous.total);
                        if ["current", "total", "workers"]
                            .iter()
                            .all(|k| v.get(k).is_none_or(|n| n.as_u64().is_some()))
                        {
                            if current <= total {
                                let event = v["event"].as_str().map(str::to_owned).or_else(|| {
                                    if name == "scan" && current == total {
                                        Some("done".into())
                                    } else {
                                        None
                                    }
                                });
                                self.phases.insert(
                                    name.into(),
                                    Phase {
                                        current,
                                        total,
                                        workers: v["workers"].as_u64().or(previous.workers),
                                        event,
                                        reason: v["reason"].as_str().map(str::to_owned),
                                        planned: v["planned"].as_u64(),
                                    },
                                );
                                self.active_phase = Some(name.into());
                            }
                        }
                    }
                }
            }
            return;
        }
        if let Some(s) = line.strip_prefix("SUMMARY ") {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                if v.is_object()
                    && [
                        "source_count",
                        "included_count",
                        "skipped_count",
                        "unreadable_count",
                    ]
                    .iter()
                    .all(|k| v.get(k).is_none_or(|n| n.as_u64().is_some()))
                {
                    self.included = v["included_count"].as_u64().unwrap_or(0);
                    self.skipped = v["skipped_count"].as_u64().unwrap_or(0);
                    self.summary = Some(v)
                }
            }
            return;
        }
        let re = regex::Regex::new(r"^\[(\d+)/(\d+)\]\s+(INCLUDE|SKIP|full_scan=\S+)").unwrap();
        if let Some(c) = re.captures(line) {
            match &c[3] {
                "INCLUDE" => self.included += 1,
                "SKIP" => self.skipped += 1,
                _ => (),
            };
            if !self.structured {
                if let (Ok(current), Ok(total)) = (c[1].parse(), c[2].parse()) {
                    self.phases.insert(
                        "process".into(),
                        Phase {
                            current,
                            total,
                            ..Default::default()
                        },
                    );
                    self.active_phase = Some("process".into())
                }
            }
        }
    }
    pub fn finish(&mut self, code: i32, cancelled: bool, cleanup_error: Option<String>) {
        self.exit_code = Some(code);
        self.error = cleanup_error.clone();
        self.running = cleanup_error.is_some();
        self.cancelling = false;
        self.outcome = Some(
            if cleanup_error.is_some() {
                "cleanup-failed"
            } else if cancelled {
                "cancelled"
            } else if ![0, 2].contains(&code) || self.summary.is_none() {
                "failed"
            } else {
                let s = self.summary.as_ref().unwrap();
                let n = s["unreadable_count"].as_u64().unwrap_or(0);
                let sources = s["source_count"].as_u64().unwrap_or(0);
                if n > 0 {
                    if sources > 0 && n >= sources {
                        "all-unreadable"
                    } else {
                        "partial-unreadable"
                    }
                } else if self.included > 0 {
                    "success"
                } else {
                    "no-events"
                }
            }
            .into(),
        );
    }
}

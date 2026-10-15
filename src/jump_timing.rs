//! Paired hyperspace journal timing and its append-only observation log.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::journal::JournalEvent;

pub const FILE_NAME: &str = "hyperspace-jumps.jsonl";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JumpMilestone {
    pub event: String,
    pub timestamp_utc: String,
    pub offset_seconds: f64,
    pub source_file: String,
    pub byte_offset: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JumpTimingObservation {
    pub version: u32,
    pub start_timestamp_utc: String,
    pub end_timestamp_utc: String,
    pub duration_seconds: f64,
    pub origin_system: Option<String>,
    pub destination_system: Option<String>,
    pub jump_distance_ly: Option<f64>,
    pub boost_used: Option<bool>,
    #[serde(default)]
    pub audio_route_offset_seconds: Option<f32>,
    #[serde(default)]
    pub analysis_start_seconds: Option<f64>,
    #[serde(default)]
    pub analysis_end_seconds: Option<f64>,
    pub milestones: Vec<JumpMilestone>,
}

impl JumpTimingObservation {
    fn identity(&self) -> Option<(String, u64)> {
        self.milestones
            .first()
            .map(|event| (event.source_file.clone(), event.byte_offset))
    }
}

struct ActiveJump {
    start: DateTime<Utc>,
    origin_system: Option<String>,
    destination_system: Option<String>,
    milestones: Vec<JumpMilestone>,
}

#[derive(Default)]
pub struct JumpTimingTracker {
    current_system: Option<String>,
    active: Option<ActiveJump>,
}

impl JumpTimingTracker {
    pub fn consume(&mut self, event: &JournalEvent) -> Option<JumpTimingObservation> {
        let value = serde_json::from_str::<serde_json::Value>(&event.raw_json).ok()?;
        let stamp_text = event.timestamp_utc.as_deref()?;
        let stamp = DateTime::parse_from_rfc3339(stamp_text)
            .ok()?
            .with_timezone(&Utc);

        if event.event == "StartJump" {
            // Any new jump supersedes an incomplete hyperspace transition.
            // The journal corpus includes aborted starts followed by a
            // supercruise StartJump; pairing those with a later FSDJump would
            // manufacture a very long observation.
            self.active = None;
        }
        if event.event == "StartJump"
            && value.get("JumpType").and_then(|v| v.as_str()) == Some("Hyperspace")
        {
            let destination = value
                .get("StarSystem")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            self.active = Some(ActiveJump {
                start: stamp,
                origin_system: self.current_system.clone(),
                destination_system: destination,
                milestones: vec![milestone(event, stamp, stamp)],
            });
            return None;
        }

        let mut completed = None;
        if let Some(active) = self.active.as_mut()
            && matches!(
                event.event.as_str(),
                "ShipLocker" | "Music" | "FSDTarget" | "ReceiveText" | "FSDJump"
            )
            && !active
                .milestones
                .iter()
                .any(|item| item.event == event.event)
        {
            active
                .milestones
                .push(milestone(event, active.start, stamp));
        }
        if event.event == "FSDJump"
            && let Some(active) = self.active.take()
        {
            let destination = value
                .get("StarSystem")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or(active.destination_system);
            completed = Some(JumpTimingObservation {
                version: 1,
                start_timestamp_utc: active.start.to_rfc3339(),
                end_timestamp_utc: stamp.to_rfc3339(),
                duration_seconds: (stamp - active.start).num_milliseconds() as f64 / 1000.0,
                origin_system: active.origin_system,
                destination_system: destination,
                jump_distance_ly: value.get("JumpDist").and_then(|v| v.as_f64()),
                boost_used: value.get("BoostUsed").and_then(|v| v.as_bool()),
                audio_route_offset_seconds: None,
                analysis_start_seconds: None,
                analysis_end_seconds: None,
                milestones: active.milestones,
            });
        }

        if matches!(event.event.as_str(), "Location" | "FSDJump" | "CarrierJump") {
            self.current_system = value
                .get("StarSystem")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or_else(|| self.current_system.clone());
        }
        completed
    }
}

fn milestone(event: &JournalEvent, start: DateTime<Utc>, stamp: DateTime<Utc>) -> JumpMilestone {
    JumpMilestone {
        event: event.event.clone(),
        timestamp_utc: stamp.to_rfc3339(),
        offset_seconds: (stamp - start).num_milliseconds() as f64 / 1000.0,
        source_file: event.source_file.clone(),
        byte_offset: event.byte_offset,
    }
}

pub struct JumpTimingStore {
    path: PathBuf,
    identities: HashSet<(String, u64)>,
    durations: Vec<f64>,
}

impl JumpTimingStore {
    pub fn path_for_config(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(FILE_NAME)
    }

    pub fn load(path: PathBuf) -> Result<Self> {
        let mut store = Self {
            path,
            identities: HashSet::new(),
            durations: Vec::new(),
        };
        if !store.path.exists() {
            return Ok(store);
        }
        let file = std::fs::File::open(&store.path)
            .with_context(|| format!("opening jump timing log {}", store.path.display()))?;
        for line in BufReader::new(file).lines() {
            let line = line.with_context(|| format!("reading {}", store.path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            let observation: JumpTimingObservation = serde_json::from_str(&line)
                .with_context(|| format!("parsing jump timing log {}", store.path.display()))?;
            if observation.version == 1
                && let Some(identity) = observation.identity()
            {
                store.identities.insert(identity);
                store.durations.push(observation.duration_seconds);
            }
        }
        Ok(store)
    }

    /// Classification based only on earlier observations. One-second journal
    /// timestamps make 18 and 19 seconds one nominal cluster, so retain at
    /// least three seconds of headroom even when MAD is zero.
    pub fn is_outlier(&self, duration_seconds: f64) -> bool {
        if self.durations.len() < 20 {
            return false;
        }
        let baseline_median = median(&self.durations);
        let deviations: Vec<f64> = self
            .durations
            .iter()
            .map(|v| (v - baseline_median).abs())
            .collect();
        duration_seconds > baseline_median + (3.0_f64 * median(&deviations)).max(3.0_f64)
    }

    pub fn append(&mut self, observation: &JumpTimingObservation) -> Result<bool> {
        let Some(identity) = observation.identity() else {
            return Ok(false);
        };
        if self.identities.contains(&identity) {
            return Ok(false);
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating jump timing directory {}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("opening jump timing log {}", self.path.display()))?;
        serde_json::to_writer(&mut file, observation)
            .context("serializing jump timing observation")?;
        file.write_all(b"\n")
            .context("writing jump timing observation")?;
        file.flush().context("flushing jump timing observation")?;
        self.identities.insert(identity);
        self.durations.push(observation.duration_seconds);
        Ok(true)
    }
}

fn median(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(offset: u64, json: &str) -> JournalEvent {
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        JournalEvent {
            timestamp_utc: value["timestamp"].as_str().map(str::to_owned),
            event: value["event"].as_str().unwrap().into(),
            source_file: "Journal.test.log".into(),
            byte_offset: offset,
            raw_json: json.into(),
        }
    }

    #[test]
    fn tracker_pairs_boundaries_and_records_milestones() {
        let mut tracker = JumpTimingTracker::default();
        tracker.consume(&event(
            1,
            r#"{"timestamp":"2026-08-27T00:00:00Z","event":"Location","StarSystem":"Sol"}"#,
        ));
        tracker.consume(&event(2, r#"{"timestamp":"2026-08-27T00:01:00Z","event":"StartJump","JumpType":"Hyperspace","StarSystem":"Achenar"}"#));
        tracker.consume(&event(
            3,
            r#"{"timestamp":"2026-08-27T00:01:05Z","event":"ShipLocker"}"#,
        ));
        let done = tracker.consume(&event(4, r#"{"timestamp":"2026-08-27T00:01:18Z","event":"FSDJump","StarSystem":"Achenar","JumpDist":139.5}"#)).unwrap();
        assert_eq!(done.duration_seconds, 18.0);
        assert_eq!(done.origin_system.as_deref(), Some("Sol"));
        assert_eq!(done.destination_system.as_deref(), Some("Achenar"));
        assert_eq!(done.milestones.len(), 3);
    }

    #[test]
    fn another_start_jump_aborts_an_incomplete_hyperspace_observation() {
        let mut tracker = JumpTimingTracker::default();
        tracker.consume(&event(
            1,
            r#"{"timestamp":"2026-08-27T00:01:00Z","event":"StartJump","JumpType":"Hyperspace","StarSystem":"Achenar"}"#,
        ));
        tracker.consume(&event(
            2,
            r#"{"timestamp":"2026-08-27T00:01:10Z","event":"StartJump","JumpType":"Supercruise"}"#,
        ));
        assert!(
            tracker
                .consume(&event(
                    3,
                    r#"{"timestamp":"2026-08-27T00:01:30Z","event":"FSDJump","StarSystem":"Elsewhere"}"#,
                ))
                .is_none()
        );
    }

    #[test]
    fn robust_outlier_rule_waits_for_a_baseline() {
        let path =
            std::env::temp_dir().join(format!("ed-compass-jumps-{}.jsonl", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut store = JumpTimingStore::load(path.clone()).unwrap();
        assert!(!store.is_outlier(40.0));
        for index in 0..20 {
            let mut observation = JumpTimingObservation {
                version: 1,
                start_timestamp_utc: "s".into(),
                end_timestamp_utc: "e".into(),
                duration_seconds: if index % 2 == 0 { 18.0 } else { 19.0 },
                origin_system: None,
                destination_system: None,
                jump_distance_ly: None,
                boost_used: None,
                audio_route_offset_seconds: None,
                analysis_start_seconds: None,
                analysis_end_seconds: None,
                milestones: vec![JumpMilestone {
                    event: "StartJump".into(),
                    timestamp_utc: "s".into(),
                    offset_seconds: 0.0,
                    source_file: "f".into(),
                    byte_offset: index,
                }],
            };
            store.append(&observation).unwrap();
            observation.milestones.clear();
        }
        assert!(!store.is_outlier(21.0));
        assert!(store.is_outlier(22.0));
        let loaded = JumpTimingStore::load(path.clone()).unwrap();
        assert!(loaded.is_outlier(22.0));
        fs::remove_file(path).unwrap();
    }
}

//! Persistent, display-only waterfall viewport presets.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::ui::waterfall::FrequencyAxis;

pub const FILE_NAME: &str = "view-presets.toml";
const SCHEMA_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewPreset {
    pub name: String,
    pub time_seconds: f32,
    pub min_hz: f32,
    pub max_hz: f32,
    /// Missing in files written before selectable projections were introduced.
    #[serde(default)]
    pub frequency_axis: FrequencyAxis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetFile {
    version: u32,
    presets: Vec<ViewPreset>,
}

#[derive(Debug)]
pub struct PresetStore {
    path: PathBuf,
    presets: Vec<ViewPreset>,
}

impl PresetStore {
    pub fn path_for_config(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join(FILE_NAME)
    }

    pub fn load(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path,
                presets: Vec::new(),
            });
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading view presets from {}", path.display()))?;
        let file: PresetFile = toml::from_str(&text)
            .with_context(|| format!("parsing view presets from {}", path.display()))?;
        if file.version != SCHEMA_VERSION {
            bail!(
                "unsupported view preset schema {} in {} (expected {})",
                file.version,
                path.display(),
                SCHEMA_VERSION
            );
        }
        validate_collection(&file.presets)?;
        Ok(Self {
            path,
            presets: file.presets,
        })
    }

    pub fn empty(path: PathBuf) -> Self {
        Self {
            path,
            presets: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn presets(&self) -> &[ViewPreset] {
        &self.presets
    }

    pub fn find(&self, name: &str) -> Option<&ViewPreset> {
        self.presets
            .iter()
            .find(|preset| same_name(&preset.name, name))
    }

    pub fn create(&mut self, mut preset: ViewPreset) -> Result<()> {
        preset.name = normalize_name(&preset.name)?;
        validate_preset(&preset)?;
        if self.find(&preset.name).is_some() {
            bail!("a view preset named {:?} already exists", preset.name);
        }
        let mut next = self.presets.clone();
        next.push(preset);
        sort_presets(&mut next);
        self.commit(next)
    }

    pub fn update(&mut self, name: &str, mut replacement: ViewPreset) -> Result<()> {
        replacement.name = normalize_name(&replacement.name)?;
        validate_preset(&replacement)?;
        let Some(index) = self
            .presets
            .iter()
            .position(|preset| same_name(&preset.name, name))
        else {
            bail!("view preset {name:?} no longer exists");
        };
        if self
            .presets
            .iter()
            .enumerate()
            .any(|(other, preset)| other != index && same_name(&preset.name, &replacement.name))
        {
            bail!("a view preset named {:?} already exists", replacement.name);
        }
        let mut next = self.presets.clone();
        next[index] = replacement;
        sort_presets(&mut next);
        self.commit(next)
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        let Some(existing) = self.find(old_name).cloned() else {
            bail!("view preset {old_name:?} no longer exists");
        };
        self.update(
            old_name,
            ViewPreset {
                name: normalize_name(new_name)?,
                ..existing
            },
        )
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        let Some(index) = self
            .presets
            .iter()
            .position(|preset| same_name(&preset.name, name))
        else {
            bail!("view preset {name:?} no longer exists");
        };
        let mut next = self.presets.clone();
        next.remove(index);
        self.commit(next)
    }

    fn commit(&mut self, next: Vec<ViewPreset>) -> Result<()> {
        write_file(&self.path, &next)?;
        self.presets = next;
        Ok(())
    }
}

pub fn normalize_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("enter a preset name");
    }
    if name.chars().count() > 64 {
        bail!("preset names must be 64 characters or fewer");
    }
    if name.eq_ignore_ascii_case("custom") {
        bail!("Custom is reserved for an unsaved view");
    }
    Ok(name.to_owned())
}

fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.trim().to_lowercase()
}

fn validate_preset(preset: &ViewPreset) -> Result<()> {
    normalize_name(&preset.name)?;
    if !preset.time_seconds.is_finite() || !(5.0..=140.0).contains(&preset.time_seconds) {
        bail!("preset time must be between 5 and 140 seconds");
    }
    if !preset.min_hz.is_finite()
        || !preset.max_hz.is_finite()
        || preset.min_hz <= 0.0
        || preset.max_hz <= preset.min_hz
        || (preset.max_hz / preset.min_hz).log2() < 0.5 - f32::EPSILON
    {
        bail!("preset frequency range must span at least half an octave");
    }
    Ok(())
}

fn validate_collection(presets: &[ViewPreset]) -> Result<()> {
    for (index, preset) in presets.iter().enumerate() {
        validate_preset(preset).with_context(|| format!("invalid preset at index {index}"))?;
        if presets[..index]
            .iter()
            .any(|other| same_name(&other.name, &preset.name))
        {
            bail!("duplicate view preset name {:?}", preset.name);
        }
    }
    Ok(())
}

fn sort_presets(presets: &mut [ViewPreset]) {
    presets.sort_by_key(|preset| preset.name.to_lowercase());
}

fn write_file(path: &Path, presets: &[ViewPreset]) -> Result<()> {
    validate_collection(presets)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("creating view preset directory {}", parent.display()))?;
    let text = toml::to_string_pretty(&PresetFile {
        version: SCHEMA_VERSION,
        presets: presets.to_vec(),
    })
    .context("serializing view presets")?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{FILE_NAME}.tmp-{}-{sequence}",
        std::process::id()
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .with_context(|| format!("creating temporary preset file {}", temp.display()))?;
        file.write_all(text.as_bytes())
            .with_context(|| format!("writing temporary preset file {}", temp.display()))?;
        file.sync_all()
            .with_context(|| format!("flushing temporary preset file {}", temp.display()))?;
        replace_file(&temp, path)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, destination: &Path) -> Result<()> {
    fs::rename(temp, destination).with_context(|| {
        format!(
            "replacing view preset file {} with {}",
            destination.display(),
            temp.display()
        )
    })
}

#[cfg(windows)]
fn replace_file(temp: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVE_FILE_FLAGS, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows::core::PCWSTR;

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let source = wide(temp);
    let target = wide(destination);
    let flags: MOVE_FILE_FLAGS = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    unsafe { MoveFileExW(PCWSTR(source.as_ptr()), PCWSTR(target.as_ptr()), flags) }
        .with_context(|| format!("replacing view preset file {}", destination.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ed-compass-presets-{tag}-{}-{sequence}.toml",
            std::process::id(),
        ))
    }

    fn preset(name: &str) -> ViewPreset {
        ViewPreset {
            name: name.into(),
            time_seconds: 35.0,
            min_hz: 200.0,
            max_hz: 2_400.0,
            frequency_axis: FrequencyAxis::Logarithmic,
        }
    }

    #[test]
    fn create_round_trips_and_sorts_names() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Titan Signal")).unwrap();
        store.create(preset("Landscape Signal")).unwrap();
        let loaded = PresetStore::load(path.clone()).unwrap();
        assert_eq!(loaded.presets()[0].name, "Landscape Signal");
        assert_eq!(loaded.presets()[1].name, "Titan Signal");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn projection_round_trips_and_legacy_files_default_to_logarithmic() {
        let path = temp_path("projection");
        let _ = fs::remove_file(&path);
        let mut store = PresetStore::empty(path.clone());
        let mut linear = preset("Linear inspection");
        linear.frequency_axis = FrequencyAxis::Linear;
        store.create(linear).unwrap();
        assert_eq!(
            PresetStore::load(path.clone()).unwrap().presets()[0].frequency_axis,
            FrequencyAxis::Linear
        );

        fs::write(
            &path,
            "version = 1\n\n[[presets]]\nname = \"Legacy\"\ntime_seconds = 35.0\nmin_hz = 200.0\nmax_hz = 2400.0\n",
        )
        .unwrap();
        assert_eq!(
            PresetStore::load(path.clone()).unwrap().presets()[0].frequency_axis,
            FrequencyAxis::Logarithmic
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn names_are_trimmed_reserved_bounded_and_case_insensitively_unique() {
        assert_eq!(
            normalize_name("  Landscape Signal  ").unwrap(),
            "Landscape Signal"
        );
        assert!(normalize_name(" ").is_err());
        assert!(normalize_name("custom").is_err());
        assert!(normalize_name(&"x".repeat(65)).is_err());
        let path = temp_path("names");
        let _ = fs::remove_file(&path);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Landscape Signal")).unwrap();
        assert!(store.create(preset("landscape signal")).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rename_never_overwrites_another_preset() {
        let path = temp_path("rename");
        let _ = fs::remove_file(&path);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Landscape Signal")).unwrap();
        store.create(preset("Titan Signal")).unwrap();
        assert!(store.rename("Titan Signal", "landscape signal").is_err());
        assert!(store.find("Titan Signal").is_some());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn update_and_delete_persist() {
        let path = temp_path("mutations");
        let _ = fs::remove_file(&path);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Landscape Signal")).unwrap();
        let mut replacement = preset("Landscape Signal");
        replacement.time_seconds = 70.0;
        store.update("Landscape Signal", replacement).unwrap();
        assert_eq!(
            PresetStore::load(path.clone()).unwrap().presets()[0].time_seconds,
            70.0
        );
        store.delete("Landscape Signal").unwrap();
        assert!(
            PresetStore::load(path.clone())
                .unwrap()
                .presets()
                .is_empty()
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_persistence_does_not_mutate_memory() {
        let parent_file = temp_path("blocked-parent");
        let _ = fs::remove_file(&parent_file);
        fs::write(&parent_file, "not a directory").unwrap();
        let mut store = PresetStore::empty(parent_file.join(FILE_NAME));
        assert!(store.create(preset("Landscape Signal")).is_err());
        assert!(store.presets().is_empty());
        fs::remove_file(parent_file).unwrap();
    }

    #[test]
    fn invalid_updates_preserve_memory_and_the_previous_file() {
        let path = temp_path("invalid-update");
        let _ = fs::remove_file(&path);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Landscape Signal")).unwrap();
        let before = fs::read_to_string(&path).unwrap();
        let mut invalid = preset("Landscape Signal");
        invalid.time_seconds = 1.0;
        assert!(store.update("Landscape Signal", invalid).is_err());
        assert_eq!(store.presets()[0].time_seconds, 35.0);
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_deletes_leave_the_preset_present() {
        let root = temp_path("failed-delete").with_extension("");
        let path = root.join(FILE_NAME);
        let _ = fs::remove_dir_all(&root);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Landscape Signal")).unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
        fs::write(&root, "blocks directory recreation").unwrap();
        assert!(store.delete("Landscape Signal").is_err());
        assert!(store.find("Landscape Signal").is_some());
        fs::remove_file(root).unwrap();
    }

    #[test]
    fn a_missing_parent_directory_is_recreated() {
        let root = temp_path("missing-parent").with_extension("");
        let path = root.join(FILE_NAME);
        let _ = fs::remove_dir_all(&root);
        let mut store = PresetStore::empty(path.clone());
        store.create(preset("Landscape Signal")).unwrap();
        assert!(path.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_or_future_files_are_preserved_and_rejected() {
        let path = temp_path("malformed");
        fs::write(&path, "this is not toml = [").unwrap();
        assert!(PresetStore::load(path.clone()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "this is not toml = [");
        fs::write(&path, "version = 999\npresets = []\n").unwrap();
        assert!(PresetStore::load(path.clone()).is_err());
        assert!(fs::read_to_string(&path).unwrap().contains("999"));
        fs::remove_file(path).unwrap();
    }
}

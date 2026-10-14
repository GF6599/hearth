//! The preferences a user keeps between runs: the scene to start, the
//! repository to type, and the speed of each scene.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::scene::{ALL, Rate, SceneKind};
use crate::scenes::typing::Speed;

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub scene: Option<SceneKind>,
    pub repo: Option<String>,
    pub typing: Option<Speed>,
    /// One for each scene in `ALL`, and `None` where the file sets none.
    pub rates: [Option<Rate>; ALL.len()],
}

pub fn default_path() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("config.toml")
}

/// The `hearth` folder under the base folder that `variable` names, or
/// under `fallback` in the home folder. The XDG spec reads an empty
/// variable as an unset one.
pub fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .filter(|base| !base.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .unwrap_or_else(std::env::temp_dir)
        .join("hearth")
}

impl Config {
    /// A missing file gives the defaults and is not created. Every other
    /// fault is an error that names the file and, where there is one, the
    /// key, so it can stop startup before the terminal opens.
    pub fn load(path: &Path) -> Result<Self, String> {
        let fail = |message: String| format!("{}: {message}", path.display());
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(fail(e.to_string())),
        };
        let document: DocumentMut = text.parse().map_err(|e| fail(format!("{e}")))?;
        let mut config = Self::default();
        for (key, item) in document.iter() {
            match key {
                "scene" => {
                    let kind = item.as_str().and_then(SceneKind::from_name);
                    config.scene =
                        Some(kind.ok_or_else(|| {
                            fail(format!("scene must be one of {}", scene_names()))
                        })?);
                }
                "repo" => {
                    let repo = item
                        .as_str()
                        .ok_or_else(|| fail("repo must be a string".into()))?;
                    config.repo = Some(repo.to_string());
                }
                "speed" => config.read_speeds(item).map_err(fail)?,
                other => {
                    return Err(fail(format!(
                        "{other} is not a setting. The settings are scene, repo and speed"
                    )));
                }
            }
        }
        Ok(config)
    }

    fn read_speeds(&mut self, item: &Item) -> Result<(), String> {
        let table = item
            .as_table_like()
            .ok_or_else(|| "speed must be a table of scene names".to_string())?;
        for (name, value) in table.iter() {
            if name == SceneKind::Typing.name() {
                let speed = value
                    .as_integer()
                    .ok_or_else(Speed::range_message)
                    .and_then(Speed::from_cps);
                self.typing = Some(speed.map_err(|e| format!("speed.{name}: {e}"))?);
                continue;
            }
            let Some(kind) = SceneKind::from_name(name) else {
                return Err(format!(
                    "speed.{name} is not a scene. The scenes are {}",
                    scene_names()
                ));
            };
            let times = value
                .as_float()
                .or_else(|| value.as_integer().map(|whole| whole as f64));
            let rate = times
                .and_then(Rate::from_times)
                .ok_or_else(|| format!("speed.{name} must be one of {}", Rate::all_times()))?;
            if let Some(slot) = ALL.iter().position(|each| *each == kind) {
                self.rates[slot] = Some(rate);
            }
        }
        Ok(())
    }
}

/// Sets one speed in the file and keeps everything else in it, comments
/// and key order included. A missing file or folder is created.
pub fn save_speed(path: &Path, scene: SceneKind, speed: impl Into<Value>) -> io::Result<()> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut document: DocumentMut = text
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let table = document
        .entry("speed")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "speed is not a table"))?;
    let mut speed = speed.into();
    // Replaced in place, because a new key would drop the comment above
    // it and the one after the old value.
    match table.get_mut(scene.name()) {
        Some(item) => {
            if let Some(old) = item.as_value() {
                *speed.decor_mut() = old.decor().clone();
            }
            *item = Item::Value(speed);
        }
        None => {
            table.insert(scene.name(), Item::Value(speed));
        }
    }
    replace(path, &document.to_string())
}

/// The new text goes to a file beside the old one and is renamed over it.
/// A rename within one folder is atomic, so a crash leaves the old file or
/// the new one and never half of either.
fn replace(path: &Path, text: &str) -> io::Result<()> {
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(folder)?;
    let temp = folder.join(format!(".config.toml.{}.tmp", std::process::id()));
    let written = fs::write(&temp, text).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

fn scene_names() -> String {
    ALL.iter()
        .map(|kind| kind.name())
        .collect::<Vec<_>>()
        .join(", ")
}

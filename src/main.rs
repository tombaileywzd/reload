use anyhow::{anyhow, Context, Result};
use globset::{Glob, GlobMatcher};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use serde::{de, Deserialize};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use std::{fs, process, thread};

fn main() {
    let reload_config_file = Path::new("config.yaml");
    let config = Config::from_file(reload_config_file).expect("Failed to load config");

    if !config.version.eq("0") {
        panic!("Invalid reload config version.");
    }

    if !config.paths.is_empty() {
        let threads = config.paths.iter().map(|path| {
            let cloned_path = path.clone();
            thread::spawn(move || {
                watch(cloned_path.clone()).expect(&format!("Failed to watch {}", cloned_path.path));
            })
        });
        for thread in threads {
            thread.join().unwrap_err();
        }
    }
}

fn watch(path_config: PathConfig) -> Result<()> {
    let mut process_executor = SingletonProcessActionExecutor::new(
        path_config.clone().action,
        path_config.clone().working_directory,
    );

    let (sender, receiver) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(200), sender)?;

    debouncer
        .watcher()
        .watch(Path::new(&path_config.path), RecursiveMode::Recursive)?;

    println!(
        "[reload] {} {}",
        path_config.action.command,
        path_config.action.args.join(" ")
    );
    process_executor.start_or_restart()?;

    for result in receiver {
        if let Ok(events) = result {
            let matches_path = events
                .iter()
                .filter_map(|event| event.path.to_str())
                .any(|path| path_config.glob_matcher.is_match(path));
            if matches_path {
                println!("[reload] Reloading from '{}'", path_config.path);
                println!(
                    "[reload] {} {}",
                    path_config.action.command,
                    path_config.action.args.join(" ")
                );
                process_executor
                    .start_or_restart()
                    .context("Failed to start/restart")
                    .unwrap();
            }
        }
    }

    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
struct Config {
    version: String,
    paths: Vec<PathConfig>,
}

impl Config {
    fn from_file(file: &Path) -> Result<Self> {
        let file_content = fs::read_to_string(file).context("Failed to read config file")?;
        serde_yaml::from_str(&file_content).context("Failed to deserialize config file")
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Action {
    command: String,
    args: Vec<String>,
}

fn deserialize_action_yaml<'de, D>(deserializer: D) -> Result<Action, D::Error>
where
    D: de::Deserializer<'de>,
{
    let commands: Vec<String> = de::Deserialize::deserialize(deserializer)?;
    Ok(Action::try_from(commands).map_err(de::Error::custom)?)
}

impl TryFrom<Vec<String>> for Action {
    type Error = anyhow::Error;

    fn try_from(value: Vec<String>) -> std::result::Result<Self, Self::Error> {
        match (
            value.iter().next(),
            value
                .iter()
                .skip(1)
                .map(String::to_owned)
                .collect::<Vec<String>>(),
        ) {
            (Some(command), args) => Ok(Action {
                command: command.to_owned(),
                args,
            }),
            _ => Err(anyhow!("Missing command.")),
        }
    }
}

fn deserialize_glob_matcher_yaml<'de, D>(deserializer: D) -> Result<GlobMatcher, D::Error>
where
    D: de::Deserializer<'de>,
{
    let pattern: &str = de::Deserialize::deserialize(deserializer)?;
    Ok(Glob::new(pattern)
        .map_err(de::Error::custom)?
        .compile_matcher())
}

#[derive(Clone, Debug, Deserialize)]
struct PathConfig {
    path: String,
    #[serde[rename="command"]]
    #[serde(deserialize_with = "deserialize_action_yaml")]
    action: Action,
    #[serde[rename="working_dir"]]
    working_directory: Option<String>,
    #[serde[rename="pattern"]]
    #[serde(deserialize_with = "deserialize_glob_matcher_yaml")]
    glob_matcher: GlobMatcher,
}

struct SingletonProcessActionExecutor {
    action: Action,
    current_directory: Option<String>,
    process_handle: Option<process::Child>,
}

impl SingletonProcessActionExecutor {
    fn new(action: Action, current_directory: Option<String>) -> Self {
        SingletonProcessActionExecutor {
            action,
            current_directory,
            process_handle: None,
        }
    }
}

impl SingletonProcessActionExecutor {
    fn start_or_restart(&mut self) -> Result<()> {
        if let Some(process_handle) = self.process_handle.as_mut() {
            process_handle.kill()?;
            process_handle.wait()?;
        }

        let action = self.action.clone();

        let mut command = Command::new(action.command);

        self.process_handle = Some(match &self.current_directory {
            Some(current_directory) => command
                .args(action.args)
                .current_dir(current_directory)
                .spawn()?,
            None => command.args(action.args).spawn()?,
        });

        Ok(())
    }
}

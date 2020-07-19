use anyhow::{Context, Result};
use derivative::Derivative;
use log::LevelFilter;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;

#[derive(Derivative)]
#[derivative(Debug)]
#[derive(Serialize)]
pub struct Logger {
    pub level: LevelFilter,
    pub path: Option<PathBuf>,

    #[derivative(Debug = "ignore")]
    #[serde(skip_serializing)]
    handle: log4rs::Handle,
}

impl Logger {
    pub fn new() -> Result<Self> {
        let level = LevelFilter::Warn;
        let path = None;

        let config = create_config(&path, level)?;
        let handle = log4rs::init_config(config)?;
        Ok(Logger {
            path,
            level,
            handle,
        })
    }

    pub fn update_settings(&mut self, level: LevelFilter, path: Option<PathBuf>) -> Result<()> {
        let config = create_config(&path, level)?;
        self.handle.set_config(config);
        self.level = level;
        self.path = path;
        Ok(())
    }

    pub fn set_level(&mut self, level: LevelFilter) -> Result<()> {
        let config = create_config(&self.path, level)?;
        self.handle.set_config(config);
        self.level = level;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn set_path(&mut self, path: Option<PathBuf>) -> Result<()> {
        let config = create_config(&path, self.level)?;
        self.handle.set_config(config);
        self.path = path;
        Ok(())
    }
}

fn create_config(path: &Option<PathBuf>, level: LevelFilter) -> Result<Config> {
    let encoder =
        PatternEncoder::new("{date(%H:%M:%S)} {level} {thread} {file}:{line} {message}{n}");

    let mut config_builder =
        Config::builder().logger(log4rs::config::Logger::builder().build("languageclient", level));

    let mut root_builder = Root::builder();
    if let Some(path) = path {
        let path = shellexpand::tilde(&path.to_string_lossy()).to_string();

        // Ensure log file writable.
        {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .with_context(|| format!("Failed to open file ({})", path))?;
            #[allow(clippy::write_literal)]
            writeln!(
                f,
                "#######\nLanguageClient {}\n#######",
                env!("CARGO_PKG_VERSION"),
            )?;
        }

        let appender = FileAppender::builder()
            .encoder(Box::new(encoder))
            .build(path)?;
        config_builder =
            config_builder.appender(Appender::builder().build("logfile", Box::new(appender)));
        root_builder = root_builder.appender("logfile");
    }
    let config = config_builder.build(root_builder.build(level))?;
    Ok(config)
}

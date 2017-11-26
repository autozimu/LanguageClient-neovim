use std;
use std::fmt;
use log::LogLevel;
use env_logger;
use chrono;
use colored::Colorize;
use types::*;

struct ColoredLevel(LogLevel);

impl fmt::Display for ColoredLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            LogLevel::Error => format!("{}", self.0).red(),
            LogLevel::Warn => format!("{}", self.0).yellow(),
            _ => format!("{}", self.0).white(),
        }.fmt(f)
    }
}

pub fn init() -> Result<()> {
    env_logger::LogBuilder::new()
        .format(|record| {
            format!(
                "{} {:14} [{}] {}",
                chrono::Local::now().format("%M:%S"),
                std::thread::current().name().unwrap_or_default(),
                ColoredLevel(record.level()),
                record.args()
            )
        })
        .parse(&std::env::var("RUST_LOG").unwrap_or_default())
        .init()?;
    Ok(())
}

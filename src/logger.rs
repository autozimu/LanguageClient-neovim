use super::*;
use log::LevelFilter;
use log4rs::Handle;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Logger, Root};

fn config(level: LevelFilter) -> Result<Config> {
    let logfile = FileAppender::builder().build(utils::get_logpath())?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .logger(Logger::builder().build("languageclient", level))
        .build(Root::builder().appender("logfile").build(level))?;
    Ok(config)
}

pub fn init() -> Result<Handle> {
    {
        // Truncate log files.
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(utils::get_logpath())?;
        writeln!(f, "")?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(utils::get_logpath_server())?;
        writeln!(f, "")?;
    }

    let handle = log4rs::init_config(config(LevelFilter::Warn)?)?;

    Ok(handle)
}

pub fn set_logging_level(handle: &Handle, level: &str) -> Result<()> {
    let level = match level.to_uppercase().as_str() {
        "DEBUG" => LevelFilter::Debug,
        "INFO" => LevelFilter::Info,
        "WARNING" | "WARN" => LevelFilter::Warn,
        "ERROR" => LevelFilter::Error,
        _ => bail!("Unknown logging level: {}", level),
    };

    let config = config(level)?;
    handle.set_config(config);
    Ok(())
}

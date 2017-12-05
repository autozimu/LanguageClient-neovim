use utils;
use log4rs;
use types::*;
use log::LogLevelFilter;
use log4rs::Handle;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Logger, Root};

fn config(level: LogLevelFilter) -> Result<Config> {
    let logfile = FileAppender::builder().build(utils::get_logpath())?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .logger(Logger::builder().build("languageclient", LogLevelFilter::Info))
        .build(
            Root::builder()
                .appender("logfile")
                .build(LogLevelFilter::Info),
        )?;
    Ok(config)
}

pub fn init() -> Result<Handle> {
    let handle = log4rs::init_config(config(LogLevelFilter::Warn)?)?;

    Ok(handle)
}

// TODO: Set loglevel at runtime.
pub fn set_loglevel(handle: Handle, level: LogLevelFilter) -> Result<()> {
    let config = config(level)?;
    handle.set_config(config);
    Ok(())
}

use super::*;
use log::LevelFilter;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::Handle;

fn create_config(path: &Option<String>, level: &LevelFilter) -> Result<Config> {
    let encoder =
        PatternEncoder::new("{date(%H:%M:%S)} {level} {thread} {file}:{line} {message}{n}");

    let mut config_builder =
        Config::builder().logger(Logger::builder().build("languageclient", *level));

    let mut root_builder = Root::builder();
    if let Some(path) = path {
        let appender = FileAppender::builder()
            .encoder(Box::new(encoder))
            .append(false)
            .build(path)?;
        config_builder =
            config_builder.appender(Appender::builder().build("logfile", Box::new(appender)));
        root_builder = root_builder.appender("logfile");
    }
    let config = config_builder.build(root_builder.build(*level))?;
    Ok(config)
}

pub fn init() -> Result<Handle> {
    let handle = log4rs::init_config(create_config(&None, &LevelFilter::Warn)?)?;

    Ok(handle)
}

pub fn update_settings(handle: &Handle, path: &Option<String>, level: &LevelFilter) -> Result<()> {
    let config = create_config(path, level)?;
    handle.set_config(config);
    Ok(())
}

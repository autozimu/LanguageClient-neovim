use super::*;
use log::LevelFilter;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::Handle;

fn create_config(path: &Option<String>, level: LevelFilter) -> Fallible<Config> {
    let encoder =
        PatternEncoder::new("{date(%H:%M:%S)} {level} {thread} {file}:{line} {message}{n}");

    let mut config_builder =
        Config::builder().logger(Logger::builder().build("languageclient", level));

    let mut root_builder = Root::builder();
    if let Some(path) = path {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let path = shellexpand::tilde(&path).to_string();

        // Ensure log file writable.
        {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .with_context(|err| format!("Failed to open file ({}): {}", path, err))?;
            #[allow(clippy::write_literal)]
            writeln!(
                f,
                "#######\nLanguageClient {} {}\n#######",
                env!("CARGO_PKG_VERSION"),
                env!("GIT_HASH")
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

pub fn init() -> Fallible<Handle> {
    let handle = log4rs::init_config(create_config(&None, LevelFilter::Info)?)?;

    Ok(handle)
}

pub fn update_settings(handle: &Handle, path: &Option<String>, level: LevelFilter) -> Fallible<()> {
    let config = create_config(path, level)?;
    handle.set_config(config);
    Ok(())
}

use crate::{language_client::LanguageClient, watcher::FsWatch};
use anyhow::Result;
use jsonrpc_core::Value;
use lsp_types::{
    notification::Notification, DidChangeWatchedFilesRegistrationOptions, RegistrationParams,
    UnregistrationParams,
};
use serde::Deserialize;
use std::{sync::mpsc, time::Duration};

#[tracing::instrument(level = "info", skip(lc))]
pub fn register_capability(
    lc: &LanguageClient,
    language_id: &str,
    params: &Value,
) -> Result<Value> {
    let params = RegistrationParams::deserialize(params)?;
    for r in &params.registrations {
        match r.method.as_str() {
            lsp_types::notification::DidChangeWatchedFiles::METHOD => {
                let opt = DidChangeWatchedFilesRegistrationOptions::deserialize(
                    r.register_options.as_ref().unwrap_or(&Value::Null),
                )?;
                if !lc.get_state(|state| state.watchers.contains_key(language_id))? {
                    let (watcher_tx, watcher_rx) = mpsc::channel();
                    // TODO: configurable duration.
                    let watcher = FsWatch::new(watcher_tx, Duration::from_secs(2))?;
                    lc.update_state(|state| {
                        state.watchers.insert(language_id.to_owned(), watcher);
                        state.watcher_rxs.insert(language_id.to_owned(), watcher_rx);
                        Ok(())
                    })?;
                }

                lc.update_state(|state| {
                    if let Some(ref mut watcher) = state.watchers.get_mut(language_id) {
                        for w in &opt.watchers {
                            log::info!("Watching glob pattern: {}", &w.glob_pattern);
                            for entry in glob::glob(&w.glob_pattern)? {
                                match entry {
                                    Ok(path) => {
                                        if path.is_dir() {
                                            watcher.watch_dir(
                                                &path,
                                                notify::RecursiveMode::Recursive,
                                            )?;
                                        } else {
                                            watcher.watch_file(&path)?;
                                        };
                                        log::info!("Start watching path {:?}", path);
                                    }
                                    Err(e) => {
                                        log::warn!("Error globbing for {}: {}", w.glob_pattern, e)
                                    }
                                }
                            }
                        }
                    }
                    Ok(())
                })?;
            }
            _ => {
                log::warn!("Unknown registration: {:?}", r);
            }
        }
    }

    lc.update_state(|state| {
        state.registrations.extend(params.registrations);
        Ok(())
    })?;
    Ok(Value::Null)
}
#[tracing::instrument(level = "info", skip(lc))]
pub fn unregister_capability(
    lc: &LanguageClient,
    language_id: &str,
    params: &Value,
) -> Result<Value> {
    let params = UnregistrationParams::deserialize(params)?;
    let mut regs_removed = vec![];
    for r in &params.unregisterations {
        if let Some(idx) = lc.get_state(|state| {
            state
                .registrations
                .iter()
                .position(|i| i.id == r.id && i.method == r.method)
        })? {
            regs_removed.push(lc.update_state(|state| Ok(state.registrations.swap_remove(idx)))?);
        }
    }

    for r in &regs_removed {
        match r.method.as_str() {
            lsp_types::notification::DidChangeWatchedFiles::METHOD => {
                let opt = DidChangeWatchedFilesRegistrationOptions::deserialize(
                    r.register_options.as_ref().unwrap_or(&Value::Null),
                )?;
                lc.update_state(|state| {
                    if let Some(ref mut watcher) = state.watchers.get_mut(language_id) {
                        for w in opt.watchers {
                            watcher.unwatch(w.glob_pattern)?;
                        }
                    }
                    Ok(())
                })?;
            }
            _ => {
                log::warn!("Unknown registration: {:?}", r);
            }
        }
    }

    Ok(Value::Null)
}

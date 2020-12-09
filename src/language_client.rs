use crate::{
    config::Config,
    types::{LanguageId, State},
    utils::diff_value,
    vim::Vim,
};
use anyhow::{anyhow, Result};
use log::*;
use serde_json::Value;
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, MutexGuard, RwLock},
};

#[derive(Clone)]
pub struct LanguageClient {
    version: String,
    state_mutex: Arc<Mutex<State>>,
    clients_mutex: Arc<Mutex<HashMap<LanguageId, Arc<Mutex<()>>>>>,
    config: Arc<RwLock<Config>>,
}

impl LanguageClient {
    pub fn new(version: impl Into<String>, state: State) -> Self {
        LanguageClient {
            version: version.into(),
            state_mutex: Arc::new(Mutex::new(state)),
            clients_mutex: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(RwLock::new(Config::default())),
        }
    }

    pub fn version(&self) -> String {
        self.version.clone()
    }

    // NOTE: Don't expose this as public.
    // MutexGuard could easily halt the program when one guard is not released immediately after use.
    fn lock(&self) -> Result<MutexGuard<State>> {
        self.state_mutex
            .lock()
            .map_err(|err| anyhow!("Failed to lock state: {:?}", err))
    }

    // This fetches a mutex that is unique to the provided languageId.
    //
    // Here, we return a mutex instead of the mutex guard because we need to satisfy the borrow
    // checker. Otherwise, there is no way to guarantee that the mutex in the hash map wouldn't be
    // garbage collected as a result of another modification updating the hash map, while something was holding the lock
    pub fn get_client_update_mutex(&self, language_id: LanguageId) -> Result<Arc<Mutex<()>>> {
        let map_guard = self.clients_mutex.lock();
        let mut map = map_guard.map_err(|err| {
            anyhow!(
                "Failed to lock client creation for languageId {:?}: {:?}",
                language_id,
                err,
            )
        })?;
        if !map.contains_key(&language_id) {
            map.insert(language_id.clone(), Arc::new(Mutex::new(())));
        }
        let mutex: Arc<Mutex<()>> = map.get(&language_id).unwrap().clone();
        Ok(mutex)
    }

    pub fn get_config<K>(&self, f: impl FnOnce(&Config) -> K) -> Result<K> {
        Ok(f(self
            .config
            .read()
            .map_err(|err| anyhow!("Failed to lock config for reading: {:?}", err))?
            .deref()))
    }

    pub fn update_config<K>(&self, f: impl FnOnce(&mut Config) -> K) -> Result<K> {
        Ok(f(self
            .config
            .write()
            .map_err(|err| anyhow!("Failed to lock config for writing: {:?}", err))?
            .deref_mut()))
    }

    pub fn get_state<T>(&self, f: impl FnOnce(&State) -> T) -> Result<T> {
        Ok(f(self.lock()?.deref()))
    }

    pub fn update_state<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut state = self.lock()?;
        let mut state = state.deref_mut();

        let v = if log_enabled!(log::Level::Debug) {
            let s = serde_json::to_string(&state)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };

        let result = f(&mut state);

        let next_v = if log_enabled!(log::Level::Debug) {
            let s = serde_json::to_string(&state)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };

        for (k, (v1, v2)) in diff_value(&v, &next_v, "state") {
            debug!("{}: {} ==> {}", k, v1, v2);
        }
        result
    }

    pub fn vim(&self) -> Result<Vim> {
        self.get_state(|state| state.vim.clone())
    }
}

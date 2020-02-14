use super::*;
use crate::vim::Vim;
use std::ops::DerefMut;

pub struct LanguageClient {
    pub version: Arc<String>,
    pub state_mutex: Arc<Mutex<State>>,
    pub clients_mutex: Arc<Mutex<HashMap<LanguageId, Arc<Mutex<()>>>>>,
}

impl LanguageClient {
    // NOTE: Don't expose this as public.
    // MutexGuard could easily halt the program when one guard is not released immediately after use.
    fn lock(&self) -> Fallible<MutexGuard<State>> {
        self.state_mutex
            .lock()
            .map_err(|err| format_err!("Failed to lock state: {:?}", err))
    }

    // This fetches a mutex that is unique to the provided languageId.
    //
    // Here, we return a mutex instead of the mutex guard because we need to satisfy the borrow
    // checker. Otherwise, there is no way to guarantee that the mutex in the hash map wouldn't be
    // garbage collected as a result of another modification updating the hash map, while something was holding the lock
    pub fn get_client_update_mutex(&self, languageId: LanguageId) -> Fallible<Arc<Mutex<()>>> {
        let map_guard = self.clients_mutex.lock();
        let mut map = map_guard.or_else(|err| {
            Err(format_err!(
                "Failed to lock client creation for languageId {:?}: {:?}",
                languageId,
                err,
            ))
        })?;
        if !map.contains_key(&languageId) {
            map.insert(languageId.clone(), Arc::new(Mutex::new(())));
        }
        let mutex: Arc<Mutex<()>> = map.get(&languageId).unwrap().clone();
        Ok(mutex)
    }

    pub fn get<T>(&self, f: impl FnOnce(&State) -> T) -> Fallible<T> {
        Ok(f(self.lock()?.deref()))
    }

    pub fn update<T>(&self, f: impl FnOnce(&mut State) -> Fallible<T>) -> Fallible<T> {
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

    pub fn vim(&self) -> Fallible<Vim> {
        self.get(|state| state.vim.clone())
    }
}

use super::*;
use crate::vim::Vim;
use std::ops::DerefMut;

pub struct LanguageClient(pub Arc<Mutex<State>>);

impl LanguageClient {
    // NOTE: Don't expose this as public.
    // MutexGuard could easily halt the program when one guard is not released immediately after use.
    fn lock(&self) -> Fallible<MutexGuard<State>> {
        self.0
            .lock()
            .map_err(|err| format_err!("Failed to lock state: {:?}", err))
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

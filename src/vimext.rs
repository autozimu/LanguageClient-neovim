use crate::language_client::LanguageClient;
use failure::Fallible;

impl LanguageClient {
    pub fn get_or_create_namespace(&self) -> Fallible<i64> {
        if let Some(namespace_id) = self.get(|state| state.namespace_id)? {
            Ok(namespace_id)
        } else {
            let namespace_id = self.vim()?.create_namespace("LanguageClient")?;
            self.update(|state| {
                state.namespace_id = Some(namespace_id);
                Ok(())
            })?;
            Ok(namespace_id)
        }
    }
}

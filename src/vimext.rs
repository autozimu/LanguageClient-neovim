use crate::language_client::LanguageClient;
use crate::types::LCNamespace;
use failure::Fallible;

impl LanguageClient {
    pub fn get_or_create_namespace(&self, ns: &LCNamespace) -> Fallible<i64> {
        let ns_name = ns.name();

        if let Some(namespace_id) = self.get(|state| state.namespace_ids.get(&ns_name).cloned())? {
            Ok(namespace_id)
        } else {
            let namespace_id = self.vim()?.create_namespace(&ns_name)?;
            self.update(|state| {
                state.namespace_ids.insert(ns_name, namespace_id);
                Ok(())
            })?;
            Ok(namespace_id)
        }
    }
}

#[cfg(test)]
use crate::rpcclient::RpcClient;
#[cfg(test)]
use crate::types::Id;
#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use serde::{de::DeserializeOwned, Serialize};

#[cfg(test)]
pub struct MockRpcClient {}

#[cfg(test)]
impl RpcClient for MockRpcClient {
    fn process_id(&self) -> Option<u32> {
        Some(0)
    }

    fn notify(&self, method: impl AsRef<str>, params: impl Serialize) -> Result<()> {
        todo!()
    }

    fn output(&self, id: Id, result: Result<impl Serialize>) -> Result<()> {
        todo!()
    }

    fn call<R: DeserializeOwned>(
        &self,
        method: impl AsRef<str>,
        params: impl Serialize,
    ) -> Result<R> {
        todo!()
    }
}

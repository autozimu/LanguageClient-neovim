pub mod io;
pub mod mock;

use crate::types::Id;
use anyhow::Result;
use io::IoRpcClient;
#[cfg(test)]
use mock::MockRpcClient;
use serde::{de::DeserializeOwned, Serialize};

pub enum Backend {
    Io(IoRpcClient),
    #[cfg(test)]
    Mock(MockRpcClient),
}

pub trait RpcClient {
    fn process_id(&self) -> Option<u32>;
    fn notify(&self, method: impl AsRef<str>, params: impl Serialize) -> Result<()>;
    fn output(&self, id: Id, result: Result<impl Serialize>) -> Result<()>;
    fn call<R: DeserializeOwned>(
        &self,
        method: impl AsRef<str>,
        params: impl Serialize,
    ) -> Result<R>;
}

impl RpcClient for Client {
    fn process_id(&self) -> Option<u32> {
        match self.backend {
            Backend::Io(ref c) => c.process_id(),
            #[cfg(test)]
            Backend::Mock(ref c) => c.process_id(),
        }
    }

    fn notify(&self, method: impl AsRef<str>, params: impl Serialize) -> Result<()> {
        match self.backend {
            Backend::Io(ref c) => c.notify(method, params),
            #[cfg(test)]
            Backend::Mock(ref c) => c.notify(method, params),
        }
    }

    fn output(&self, id: Id, result: Result<impl Serialize>) -> Result<()> {
        match self.backend {
            Backend::Io(ref c) => c.output(id, result),
            #[cfg(test)]
            Backend::Mock(ref c) => c.output(id, result),
        }
    }

    fn call<R: DeserializeOwned>(
        &self,
        method: impl AsRef<str>,
        params: impl Serialize,
    ) -> Result<R> {
        match self.backend {
            Backend::Io(ref c) => c.call(method, params),
            #[cfg(test)]
            Backend::Mock(ref c) => c.call(method, params),
        }
    }
}

pub struct Client {
    pub backend: Backend,
}

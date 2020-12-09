use crate::types::{Call, Id, LSError, LanguageId, RawMessage, ToInt, ToParams, ToRpcError};
use anyhow::{anyhow, Result};
use crossbeam::channel::{bounded, unbounded, Receiver, Sender};
use log::*;
use regex::Regex;
use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::str::FromStr;
use std::{
    collections::HashMap,
    io::BufRead,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

const CONTENT_MODIFIED_ERROR_CODE: i64 = -32801;

lazy_static! {
    // this regex is used to remove some additional fields that we get from some servers, namely:
    // meta, sent by javascript-typescript-langserver and requestMethod, sent by Sorbet.
    static ref RE_REMOVE_EXTRA_FIELDS: Regex =
        Regex::new(r#",\s?"(?:meta|requestMethod)":(?:"\w+(/\w+)?"|\{\})"#).unwrap();
}

#[derive(Serialize)]
pub struct RpcClient {
    language_id: LanguageId,
    #[serde(skip_serializing)]
    id: AtomicU64,
    #[serde(skip_serializing)]
    writer_tx: Sender<RawMessage>,
    #[serde(skip_serializing)]
    reader_tx: Sender<(Id, Sender<jsonrpc_core::Output>)>,
    pub process_id: Option<u32>,
}

impl RpcClient {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        language_id: LanguageId,
        reader: impl BufRead + Send + 'static,
        writer: impl Write + Send + 'static,
        process_id: Option<u32>,
        sink: Sender<Call>,
        on_crash: impl Fn(&LanguageId) + Clone + Send + 'static,
    ) -> Result<Self> {
        let (reader_tx, reader_rx): (Sender<(Id, Sender<jsonrpc_core::Output>)>, _) = unbounded();

        let language_id_clone = language_id.clone();
        let reader_thread_name = format!("reader-{:?}", language_id);
        let on_crash_clone = on_crash.clone();
        thread::Builder::new()
            .name(reader_thread_name.clone())
            .spawn(move || {
                if let Err(err) = loop_read(reader, reader_rx, &sink, &language_id_clone) {
                    match err.downcast_ref::<std::io::Error>() {
                        Some(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                            on_crash_clone(&language_id_clone)
                        }
                        _ => {}
                    }

                    error!("Thread {} exited with error: {:?}", reader_thread_name, err);
                }
            })?;

        let (writer_tx, writer_rx) = unbounded();
        let writer_thread_name = format!("writer-{:?}", language_id);
        let language_id_clone = language_id.clone();
        thread::Builder::new()
            .name(writer_thread_name.clone())
            .spawn(move || {
                if let Err(err) = loop_write(writer, &writer_rx, &language_id_clone) {
                    match err.downcast_ref::<std::io::Error>() {
                        Some(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                            on_crash(&language_id_clone)
                        }
                        _ => {}
                    }

                    error!("Thread {} exited with error: {:?}", writer_thread_name, err);
                }
            })?;

        Ok(Self {
            language_id,
            id: AtomicU64::default(),
            process_id,
            reader_tx,
            writer_tx,
        })
    }

    pub fn call<R: DeserializeOwned>(
        &self,
        method: impl AsRef<str>,
        params: impl Serialize,
    ) -> Result<R> {
        let method = method.as_ref();
        let id = self.id.fetch_add(1, Ordering::SeqCst);
        let msg = jsonrpc_core::MethodCall {
            jsonrpc: Some(jsonrpc_core::Version::V2),
            id: jsonrpc_core::Id::Num(id),
            method: method.to_owned(),
            params: params.to_params()?,
        };
        let (tx, rx) = bounded(1);
        self.reader_tx.send((id, tx))?;
        self.writer_tx.send(RawMessage::MethodCall(msg))?;
        // TODO: duration from config.
        match rx.recv_timeout(Duration::from_secs(60))? {
            jsonrpc_core::Output::Success(ok) => Ok(serde_json::from_value(ok.result)?),
            // NOTE: Errors with code -32801 correspond to the protocol's ContentModified error,
            // which we don't want to show to the user and should ignore, as the result of the
            // request that triggered this error has been invalidated by changes to the state
            // of the server, so we must handle this error specifically.
            jsonrpc_core::Output::Failure(err)
                if err.error.code.code() == CONTENT_MODIFIED_ERROR_CODE =>
            {
                Err(anyhow::Error::from(LSError::ContentModified))
            }
            jsonrpc_core::Output::Failure(err) => Err(anyhow!("Error: {:?}", err)),
        }
    }

    pub fn notify(&self, method: impl AsRef<str>, params: impl Serialize) -> Result<()> {
        let method = method.as_ref();

        let msg = jsonrpc_core::Notification {
            jsonrpc: Some(jsonrpc_core::Version::V2),
            method: method.to_owned(),
            params: params.to_params()?,
        };
        self.writer_tx.send(RawMessage::Notification(msg))?;
        Ok(())
    }

    pub fn output(&self, id: Id, result: Result<impl Serialize>) -> Result<()> {
        let output = match result {
            Ok(ok) => jsonrpc_core::Output::Success(jsonrpc_core::Success {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                id: jsonrpc_core::Id::Num(id),
                result: serde_json::to_value(ok)?,
            }),
            Err(err) => jsonrpc_core::Output::Failure(jsonrpc_core::Failure {
                jsonrpc: Some(jsonrpc_core::Version::V2),
                id: jsonrpc_core::Id::Num(id),
                error: err.to_rpc_error(),
            }),
        };

        self.writer_tx.send(RawMessage::Output(output))?;
        Ok(())
    }
}

fn loop_read(
    reader: impl BufRead,
    reader_rx: Receiver<(Id, Sender<jsonrpc_core::Output>)>,
    sink: &Sender<Call>,
    language_id: &LanguageId,
) -> Result<()> {
    let mut pending_outputs = HashMap::new();

    // Count how many consequent empty lines.
    let mut count_empty_lines = 0;

    let mut reader = reader;
    let mut content_length = 0;
    loop {
        let mut message = String::new();
        let mut line = String::new();
        if language_id.is_some() {
            reader.read_line(&mut line)?;
            let line = line.trim();
            if line.is_empty() {
                count_empty_lines += 1;
                if count_empty_lines > 5 {
                    return Err(anyhow!("Unable to read from language server"));
                }

                let mut buf = vec![0; content_length];
                reader.read_exact(buf.as_mut_slice())?;
                message = String::from_utf8(buf)?;
            } else {
                count_empty_lines = 0;
                if !line.starts_with("Content-Length") {
                    continue;
                }

                let tokens: Vec<&str> = line.splitn(2, ':').collect();
                let len = tokens
                    .get(1)
                    .ok_or_else(|| anyhow!("Failed to get length! tokens: {:?}", tokens))?
                    .trim();
                content_length = usize::from_str(len)?;
            }
        } else if reader.read_line(&mut message)? == 0 {
            break;
        }

        let message = message.trim();
        if message.is_empty() {
            continue;
        }
        debug!("<= {:?} {}", language_id, message);
        // FIXME: Remove extra `meta` property from javascript-typescript-langserver and
        // `requestMethod` sent by Sorbet.
        let s = RE_REMOVE_EXTRA_FIELDS.replace(message, "");
        let message = serde_json::from_str(&s);
        if let Err(ref err) = message {
            error!(
                "Failed to deserialize output: {}\n\n Message: {}\n\nError: {:?}",
                err, s, err
            );
            continue;
        }
        // TODO: cleanup.
        let message = message.unwrap();
        match message {
            RawMessage::MethodCall(method_call) => {
                sink.send(Call::MethodCall(language_id.clone(), method_call))?;
            }
            RawMessage::Notification(notification) => {
                sink.send(Call::Notification(language_id.clone(), notification))?;
            }
            RawMessage::Output(output) => {
                while let Ok((id, tx)) = reader_rx.try_recv() {
                    pending_outputs.insert(id, tx);
                }

                if let Some(tx) = pending_outputs.remove(&output.id().to_int()?) {
                    tx.send(output)
                        .map_err(|output| anyhow!("Failed to send output: {:?}", output))?;
                }
            }
        };
    }

    info!("reader-{:?} terminated", language_id);
    Ok(())
}

fn loop_write(
    writer: impl Write,
    rx: &Receiver<RawMessage>,
    language_id: &LanguageId,
) -> Result<()> {
    let mut writer = writer;

    for msg in rx.iter() {
        let s = serde_json::to_string(&msg)?;
        debug!("=> {:?} {}", language_id, s);
        if language_id.is_none() {
            // Use different convention for two reasons,
            // 1. If using '\r\ncontent', nvim will receive output as `\r` + `content`, while vim
            // receives `content`.
            // 2. Without last line ending, vim output handler won't be triggered.
            write!(writer, "Content-Length: {}\n\n{}\n", s.len(), s)?;
        } else {
            write!(writer, "Content-Length: {}\r\n\r\n{}", s.len(), s)?;
        };
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::RE_REMOVE_EXTRA_FIELDS;
    use crate::types::RawMessage;

    #[test]
    // The library we're using for json-rpc doesn't accept extra fields in the structs used to
    // deserialize the message. Sorbet (and possibly other servers) sends an extra field in it, so
    // the client fails to deserialize that response.
    // Our previous solution was to pin the dependency to jsonrpc-core to version 12, but is
    // suboptimal, so we now try to remove the extra fields we know of from the response.
    //
    // See related issue: https://github.com/autozimu/LanguageClient-neovim/issues/892
    fn it_should_remove_extra_fields() {
        // it removes the requestMethod field from Sorbet
        let message = r#"{"jsonrpc":"2.0","id":1,"requestMethod":"initialize","result":0}"#;
        let message = RE_REMOVE_EXTRA_FIELDS.replace(message, "");
        let result: Result<RawMessage, _> = serde_json::from_str(&message);
        assert!(result.is_ok());

        let message =
            r#"{"jsonrpc":"2.0","id":1,"requestMethod":"textDocument/definition","result":0}"#;
        let message = RE_REMOVE_EXTRA_FIELDS.replace(message, "");
        let result: Result<RawMessage, _> = serde_json::from_str(&message);
        assert!(result.is_ok());

        // it removes the meta field from javascript-typescript-langserver
        let message = r#"{"jsonrpc":"2.0","id":1,"meta":{},"result":0}"#;
        let message = RE_REMOVE_EXTRA_FIELDS.replace(message, "");
        let result: Result<RawMessage, _> = serde_json::from_str(&message);
        assert!(result.is_ok());
    }
}

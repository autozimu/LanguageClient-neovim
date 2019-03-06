use super::*;
use crate::types::{Call, RawMessage};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

#[derive(Clone, Serialize)]
pub struct RpcClient {
    languageId: LanguageId,
    // TODO: make atomic.
    #[serde(skip_serializing)]
    id: Arc<Mutex<Id>>,
    #[serde(skip_serializing)]
    writer_tx: Sender<RawMessage>,
    #[serde(skip_serializing)]
    reader_tx: Sender<(Id, Sender<rpc::Output>)>,
    pub process_id: Option<u32>,
}

impl RpcClient {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        languageId: LanguageId,
        reader: impl BufRead + Send + 'static,
        writer: impl Write + Send + 'static,
        process_id: Option<u32>,
        sink: Sender<Call>,
    ) -> Fallible<Self> {
        let (reader_tx, reader_rx): (Sender<(Id, Sender<rpc::Output>)>, _) = unbounded();

        let languageId_clone = languageId.clone();
        let reader_thread_name = format!("reader-{:?}", languageId);
        thread::Builder::new()
            .name(reader_thread_name.clone())
            .spawn(move || {
                if let Err(err) = loop_read(reader, reader_rx, sink, languageId_clone) {
                    error!("Thread {} exited with error: {:?}", reader_thread_name, err);
                }
            })?;

        let (writer_tx, writer_rx) = unbounded();
        let writer_thread_name = format!("writer-{:?}", languageId);
        let languageId_clone = languageId.clone();
        thread::Builder::new()
            .name(writer_thread_name.clone())
            .spawn(move || {
                if let Err(err) = loop_write(writer, writer_rx, languageId_clone) {
                    error!("Thread {} exited with error: {:?}", writer_thread_name, err);
                }
            })?;

        Ok(RpcClient {
            languageId,
            id: Arc::new(Mutex::new(0)),
            process_id,
            reader_tx,
            writer_tx,
        })
    }

    pub fn call<R: DeserializeOwned>(
        &self,
        method: impl AsRef<str>,
        params: impl Serialize,
    ) -> Fallible<R> {
        let method = method.as_ref();
        let id = {
            let mut id = self
                .id
                .lock()
                .map_err(|err| format_err!("Failed to lock msg id: {}", err))?;
            *id += 1;
            *id
        };
        let msg = rpc::MethodCall {
            jsonrpc: Some(rpc::Version::V2),
            id: rpc::Id::Num(id),
            method: method.to_owned(),
            params: params.to_params()?,
        };
        let (tx, rx) = bounded(1);
        self.reader_tx.send((id, tx))?;
        self.writer_tx.send(RawMessage::MethodCall(msg))?;
        // TODO: duration from config.
        match rx.recv_timeout(Duration::from_secs(60))? {
            rpc::Output::Success(ok) => Ok(serde_json::from_value(ok.result)?),
            rpc::Output::Failure(err) => bail!("Error: {:?}", err),
        }
    }

    pub fn notify(&self, method: impl AsRef<str>, params: impl Serialize) -> Fallible<()> {
        let method = method.as_ref();

        let msg = rpc::Notification {
            jsonrpc: Some(rpc::Version::V2),
            method: method.to_owned(),
            params: params.to_params()?,
        };
        self.writer_tx.send(RawMessage::Notification(msg))?;
        Ok(())
    }

    pub fn output(&self, id: Id, result: Fallible<impl Serialize>) -> Fallible<()> {
        let output = match result {
            Ok(ok) => rpc::Output::Success(rpc::Success {
                jsonrpc: Some(rpc::Version::V2),
                id: rpc::Id::Num(id),
                result: serde_json::to_value(ok)?,
            }),
            Err(err) => rpc::Output::Failure(rpc::Failure {
                jsonrpc: Some(rpc::Version::V2),
                id: rpc::Id::Num(id),
                error: err.to_rpc_error(),
            }),
        };

        self.writer_tx.send(RawMessage::Output(output))?;
        Ok(())
    }
}

fn loop_read(
    reader: impl BufRead,
    reader_rx: Receiver<(Id, Sender<rpc::Output>)>,
    sink: Sender<Call>,
    languageId: LanguageId,
) -> Fallible<()> {
    let mut pending_outputs = HashMap::new();

    // Count how many consequent empty lines.
    let mut count_empty_lines = 0;

    let mut reader = reader;
    let mut content_length = 0;
    loop {
        let mut message = String::new();
        let mut line = String::new();
        if languageId.is_some() {
            reader.read_line(&mut line)?;
            let line = line.trim();
            if line.is_empty() {
                count_empty_lines += 1;
                if count_empty_lines > 5 {
                    bail!("Unable to read from language server");
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
                    .ok_or_else(|| format_err!("Failed to get length! tokens: {:?}", tokens))?
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
        info!("<= {:?} {}", languageId, message);
        // FIXME: Remove extra `meta` property from javascript-typescript-langserver.
        let s = message.replace(r#","meta":{}"#, "");
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
                sink.send(Call::MethodCall(languageId.clone(), method_call))?;
            }
            RawMessage::Notification(notification) => {
                sink.send(Call::Notification(languageId.clone(), notification))?;
            }
            RawMessage::Output(output) => {
                while let Ok((id, tx)) = reader_rx.try_recv() {
                    pending_outputs.insert(id, tx);
                }

                if let Some(tx) = pending_outputs.remove(&output.id().to_int()?) {
                    tx.send(output)
                        .map_err(|output| format_err!("Failed to send output: {:?}", output))?;
                }
            }
        };
    }

    info!("reader-{:?} terminated", languageId);
    Ok(())
}

fn loop_write(
    writer: impl Write,
    rx: Receiver<RawMessage>,
    languageId: LanguageId,
) -> Fallible<()> {
    let mut writer = writer;

    for msg in rx.iter() {
        let s = serde_json::to_string(&msg)?;
        info!("=> {:?} {}", languageId, s);
        if languageId.is_none() {
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

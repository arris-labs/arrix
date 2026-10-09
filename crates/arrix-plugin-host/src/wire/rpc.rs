//! JSON-RPC 2.0 as ADR-0022 uses it: one JSON object per line, both ways,
//! params by name, either side requesting while a request of the other is
//! open. No batches, no `Content-Length`.

use std::io::{self, BufRead};

use serde_json::{Map, Value, json};

/// The wire protocol's version, which `initialize` exchanges (ADR-0022 §7).
pub const PROTOCOL_VERSION: u32 = 1;

/// JSON that does not parse.
pub const PARSE_ERROR: i64 = -32700;
/// Parses, but is no request or response of this protocol.
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
/// A method's params that do not have its schema's shape.
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
/// A line past the reader's limit (the implementation-defined range).
pub const LINE_TOO_LONG: i64 = -32001;

/// The longest line a reader takes: a refined mesh of the bracket is
/// 0.5 MB (ADR-0022), so this is generous and still bounded.
pub const MAX_LINE: usize = 16 * 1024 * 1024;

/// A request's id: the caller's to choose, echoed in the response.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RequestId {
    Number(i64),
    Text(String),
}

impl RequestId {
    fn to_json(&self) -> Value {
        match self {
            RequestId::Number(n) => json!(n),
            RequestId::Text(s) => json!(s),
        }
    }

    fn from_json(v: &Value) -> Option<RequestId> {
        match v {
            Value::Number(n) => n.as_i64().map(RequestId::Number),
            Value::String(s) => Some(RequestId::Text(s.clone())),
            _ => None,
        }
    }
}

/// The protocol's error (not a feature's or a kernel call's failure, which
/// is a `diagnostic` in a result, ADR-0022 §4).
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(METHOD_NOT_FOUND, format!("no method `{method}` here"))
    }

    pub fn invalid_params(error: &super::WireError) -> Self {
        Self::new(INVALID_PARAMS, error.to_string())
    }

    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("code".into(), json!(self.code));
        m.insert("message".into(), json!(self.message));
        if let Some(data) = &self.data {
            m.insert("data".into(), data.clone());
        }
        Value::Object(m)
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// A call. Without an id it is a notification, which gets no response.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: Option<RequestId>,
    pub method: String,
    /// An object of the method's params by name (`null` when left out).
    pub params: Value,
}

/// An answer: the result or the protocol's error. The id is none only for
/// an error to a line whose id could not be read.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub id: Option<RequestId>,
    pub outcome: Result<Value, RpcError>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Request(Request),
    Response(Response),
}

/// A line that is no message, and the error to answer it with.
#[derive(Clone, Debug, PartialEq)]
pub struct Malformed {
    /// The id, when the line had a readable one.
    pub id: Option<RequestId>,
    pub error: RpcError,
}

impl Malformed {
    fn new(id: Option<RequestId>, code: i64, message: impl Into<String>) -> Self {
        Self {
            id,
            error: RpcError::new(code, message),
        }
    }

    pub fn into_response(self) -> Response {
        Response {
            id: self.id,
            outcome: Err(self.error),
        }
    }
}

impl Message {
    pub fn request(id: i64, method: &str, params: Value) -> Message {
        Message::Request(Request {
            id: Some(RequestId::Number(id)),
            method: method.to_owned(),
            params,
        })
    }

    pub fn result(id: RequestId, result: Value) -> Message {
        Message::Response(Response {
            id: Some(id),
            outcome: Ok(result),
        })
    }

    /// The message as one line, without the newline.
    pub fn to_line(&self) -> String {
        let mut m = Map::new();
        m.insert("jsonrpc".into(), json!("2.0"));
        match self {
            Message::Request(r) => {
                if let Some(id) = &r.id {
                    m.insert("id".into(), id.to_json());
                }
                m.insert("method".into(), json!(r.method));
                if !r.params.is_null() {
                    m.insert("params".into(), r.params.clone());
                }
            }
            Message::Response(r) => {
                m.insert(
                    "id".into(),
                    r.id.as_ref().map_or(Value::Null, RequestId::to_json),
                );
                match &r.outcome {
                    Ok(result) => m.insert("result".into(), result.clone()),
                    Err(e) => m.insert("error".into(), e.to_json()),
                };
            }
        }
        Value::Object(m).to_string()
    }

    /// Reads one line.
    pub fn parse(line: &str) -> Result<Message, Malformed> {
        let value: Value = serde_json::from_str(line)
            .map_err(|e| Malformed::new(None, PARSE_ERROR, format!("not JSON: {e}")))?;
        let Value::Object(m) = value else {
            let what = if value.is_array() {
                "batches are not supported"
            } else {
                "a message is a JSON object"
            };
            return Err(Malformed::new(None, INVALID_REQUEST, what));
        };
        // The id is read first, so a later fault is answered to it.
        let id = match m.get("id") {
            None | Some(Value::Null) => None,
            Some(v) => Some(RequestId::from_json(v).ok_or_else(|| {
                Malformed::new(None, INVALID_REQUEST, "an id is an integer or a string")
            })?),
        };
        let fault = |message: &str| Malformed::new(id.clone(), INVALID_REQUEST, message);
        if m.get("jsonrpc") != Some(&json!("2.0")) {
            return Err(fault("`jsonrpc` is \"2.0\""));
        }
        if let Some(method) = m.get("method") {
            let Some(method) = method.as_str() else {
                return Err(fault("`method` is a string"));
            };
            if m.contains_key("result") || m.contains_key("error") {
                return Err(fault("a request has no `result` or `error`"));
            }
            let params = match m.get("params") {
                None | Some(Value::Null) => Value::Null,
                Some(p @ Value::Object(_)) => p.clone(),
                Some(_) => {
                    return Err(Malformed::new(
                        id.clone(),
                        INVALID_PARAMS,
                        "`params` is an object of the params by name",
                    ));
                }
            };
            return Ok(Message::Request(Request {
                id: id.clone(),
                method: method.to_owned(),
                params,
            }));
        }
        let outcome = match (m.get("result"), m.get("error")) {
            (Some(result), None) => Ok(result.clone()),
            (None, Some(error)) => Err(error_from_json(error).ok_or_else(|| {
                fault("`error` is an object with an integer `code` and a string `message`")
            })?),
            (Some(_), Some(_)) => {
                return Err(fault("a response has a `result` or an `error`, not both"));
            }
            (None, None) => {
                return Err(fault(
                    "neither a request (`method`) nor a response (`result` or `error`)",
                ));
            }
        };
        if id.is_none() && outcome.is_ok() {
            return Err(fault("a response with a result has an id"));
        }
        Ok(Message::Response(Response { id, outcome }))
    }
}

fn error_from_json(v: &Value) -> Option<RpcError> {
    let m = v.as_object()?;
    Some(RpcError {
        code: m.get("code")?.as_i64()?,
        message: m.get("message")?.as_str()?.to_owned(),
        data: m.get("data").cloned(),
    })
}

/// What a line reader can fail with.
#[derive(Debug)]
pub enum LineError {
    /// A line past the limit; it has been skipped to its newline, so the
    /// next read starts at the next line.
    TooLong {
        limit: usize,
    },
    NotUtf8,
    Io(io::Error),
}

impl LineError {
    /// The error to answer with, when the pipe is still open.
    pub fn malformed(&self) -> Option<Malformed> {
        match self {
            LineError::TooLong { limit } => Some(Malformed::new(
                None,
                LINE_TOO_LONG,
                format!("a line is at most {limit} bytes"),
            )),
            LineError::NotUtf8 => Some(Malformed::new(None, PARSE_ERROR, "a line is UTF-8")),
            LineError::Io(_) => None,
        }
    }
}

/// Newline-framed lines off a pipe, each at most `limit` bytes. A longer
/// one is read to its end and discarded rather than buffered.
pub struct LineReader<R> {
    inner: R,
    limit: usize,
}

impl<R: BufRead> LineReader<R> {
    pub fn new(inner: R) -> Self {
        Self::with_limit(inner, MAX_LINE)
    }

    pub fn with_limit(inner: R, limit: usize) -> Self {
        Self { inner, limit }
    }

    /// The next line without its newline (`\r\n` too), `None` at the end of
    /// the stream. A blank line is skipped, and a last line without a
    /// newline is a line.
    pub fn next_line(&mut self) -> Result<Option<String>, LineError> {
        loop {
            let mut line = Vec::new();
            let mut too_long = false;
            let mut ended = false;
            loop {
                let buf = self.inner.fill_buf().map_err(LineError::Io)?;
                if buf.is_empty() {
                    ended = true;
                    break;
                }
                let (chunk, found) = match buf.iter().position(|&b| b == b'\n') {
                    Some(i) => (&buf[..i], true),
                    None => (buf, false),
                };
                if !too_long {
                    if line.len() + chunk.len() > self.limit {
                        too_long = true;
                        line = Vec::new();
                    } else {
                        line.extend_from_slice(chunk);
                    }
                }
                let consumed = chunk.len() + usize::from(found);
                self.inner.consume(consumed);
                if found {
                    break;
                }
            }
            if too_long {
                return Err(LineError::TooLong { limit: self.limit });
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                if ended {
                    return Ok(None);
                }
                continue;
            }
            return String::from_utf8(line)
                .map(Some)
                .map_err(|_| LineError::NotUtf8);
        }
    }
}

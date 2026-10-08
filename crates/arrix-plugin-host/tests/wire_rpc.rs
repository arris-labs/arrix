//! The JSON-RPC framing (ADR-0022 §1, §2, §4): messages to a line and
//! back, and a malformed-input test for each error the reader and the
//! parser can answer with.

use std::io::{BufReader, Cursor, Read};

use arrix_plugin_host::wire::WireError;
use arrix_plugin_host::wire::rpc::{
    INVALID_PARAMS, INVALID_REQUEST, LINE_TOO_LONG, LineError, LineReader, Malformed, Message,
    PARSE_ERROR, Request, RequestId, Response, RpcError,
};
use serde_json::{Value, json};

fn parse(line: &str) -> Result<Message, Malformed> {
    Message::parse(line)
}

fn malformed(line: &str) -> Malformed {
    parse(line).expect_err(&format!("{line} is malformed"))
}

#[test]
fn messages_round_trip_through_a_line() {
    let call = Message::request(7, "kernel.extrude", json!({"p": {}, "distance": 0.01}));
    let notification = Message::Request(Request {
        id: None,
        method: "feature.describe".into(),
        params: Value::Null,
    });
    let ok = Message::result(RequestId::Text("a-1".into()), json!({"ok": 3}));
    let failed = Message::Response(Response {
        id: Some(RequestId::Number(8)),
        outcome: Err(RpcError {
            code: -32601,
            message: "no".into(),
            data: Some(json!({"why": "because"})),
        }),
    });
    let unreadable = Message::Response(Response {
        id: None,
        outcome: Err(RpcError::new(PARSE_ERROR, "not JSON")),
    });
    for m in [call, notification, ok, failed, unreadable] {
        let line = m.to_line();
        assert!(!line.contains('\n'), "one line: {line}");
        assert_eq!(parse(&line).unwrap(), m, "{line}");
    }
    assert_eq!(
        Message::request(1, "m", Value::Null).to_line(),
        r#"{"id":1,"jsonrpc":"2.0","method":"m"}"#
    );
}

#[test]
fn a_line_that_is_not_json_is_a_parse_error() {
    for line in [
        "",
        "not json",
        "{\"jsonrpc\": \"2.0\"",
        "{'single': 'quotes'}",
    ] {
        let m = malformed(line);
        assert_eq!((m.error.code, m.id), (PARSE_ERROR, None), "{line:?}");
    }
}

#[test]
fn json_that_is_no_message_is_an_invalid_request() {
    let cases = [
        ("3", "a message is a JSON object"),
        ("\"x\"", "a message is a JSON object"),
        ("null", "a message is a JSON object"),
        ("[]", "batches are not supported"),
        (
            r#"[{"jsonrpc":"2.0","id":1,"method":"m"}]"#,
            "batches are not supported",
        ),
        (r#"{"id":1,"method":"m"}"#, "`jsonrpc`"),
        (r#"{"jsonrpc":"1.0","id":1,"method":"m"}"#, "`jsonrpc`"),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":3}"#,
            "`method` is a string",
        ),
        (r#"{"jsonrpc":"2.0","id":1}"#, "neither a request"),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"m","result":1}"#,
            "no `result` or `error`",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":1,"error":{"code":1,"message":"m"}}"#,
            "not both",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":"x","message":"m"}}"#,
            "integer `code`",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":1}}"#,
            "string `message`",
        ),
        (r#"{"jsonrpc":"2.0","id":1,"error":3}"#, "integer `code`"),
        (r#"{"jsonrpc":"2.0","result":1}"#, "has an id"),
    ];
    for (line, what) in cases {
        let m = malformed(line);
        assert_eq!(m.error.code, INVALID_REQUEST, "{line}");
        assert!(
            m.error.message.contains(what),
            "{line}: {}",
            m.error.message
        );
    }
}

#[test]
fn an_unusable_id_is_an_invalid_request_with_no_id_to_answer_to() {
    for id in [json!(1.5), json!(true), json!([1]), json!({"a": 1})] {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": "m"}).to_string();
        let m = malformed(&line);
        assert_eq!((m.error.code, m.id), (INVALID_REQUEST, None), "{line}");
    }
}

#[test]
fn a_fault_after_the_id_is_answered_to_it() {
    let m = malformed(r#"{"jsonrpc":"2.0","id":"q","method":5}"#);
    assert_eq!(m.id, Some(RequestId::Text("q".into())));
    let answer = m.into_response();
    assert_eq!(answer.id, Some(RequestId::Text("q".into())));
    assert_eq!(answer.outcome.unwrap_err().code, INVALID_REQUEST);
}

#[test]
fn params_are_an_object_by_name() {
    for params in [json!([1, 2]), json!(3), json!("x"), json!(true)] {
        let line = json!({"jsonrpc": "2.0", "id": 1, "method": "m", "params": params}).to_string();
        let m = malformed(&line);
        assert_eq!(m.error.code, INVALID_PARAMS, "{line}");
        assert_eq!(m.id, Some(RequestId::Number(1)));
    }
    // Left out or null are both no params.
    for line in [
        r#"{"jsonrpc":"2.0","id":1,"method":"m"}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"m","params":null}"#,
    ] {
        let Message::Request(r) = parse(line).unwrap() else {
            panic!("a request");
        };
        assert_eq!(r.params, Value::Null);
    }
}

#[test]
fn bad_params_are_answered_with_where() {
    let e = RpcError::invalid_params(&WireError::new("missing field `y`").at("y").at("p"));
    assert_eq!(e.code, INVALID_PARAMS);
    assert_eq!(e.message, "p.y: missing field `y`");
}

fn lines(input: &[u8], limit: usize) -> Vec<Result<Option<String>, String>> {
    let mut reader = LineReader::with_limit(
        BufReader::with_capacity(4, Cursor::new(input.to_vec())),
        limit,
    );
    let mut out = Vec::new();
    loop {
        match reader.next_line() {
            Ok(None) => {
                out.push(Ok(None));
                return out;
            }
            Ok(line) => out.push(Ok(line)),
            Err(LineError::TooLong { limit }) => out.push(Err(format!("too long {limit}"))),
            Err(LineError::NotUtf8) => out.push(Err("not utf-8".into())),
            Err(LineError::Io(e)) => panic!("{e}"),
        }
    }
}

fn some(s: &str) -> Result<Option<String>, String> {
    Ok(Some(s.to_owned()))
}

#[test]
fn the_reader_splits_on_newlines_whatever_the_buffer() {
    // A four-byte buffer, so every line crosses fills.
    assert_eq!(
        lines(b"{\"a\":1}\n{\"b\":22}\r\n\n\n{\"c\":3}", 64),
        [
            some("{\"a\":1}"),
            some("{\"b\":22}"),
            some("{\"c\":3}"),
            Ok(None)
        ]
    );
    assert_eq!(lines(b"", 64), [Ok(None)]);
    assert_eq!(lines(b"\n\n", 64), [Ok(None)]);
    // A last line with no newline is a line.
    assert_eq!(lines(b"one\ntwo", 64), [some("one"), some("two"), Ok(None)]);
}

#[test]
fn a_line_past_the_limit_is_skipped_and_the_next_is_read() {
    let long = "x".repeat(40);
    let input = format!("ok\n{long}\nalso ok\n");
    assert_eq!(
        lines(input.as_bytes(), 16),
        [
            some("ok"),
            Err("too long 16".into()),
            some("also ok"),
            Ok(None)
        ]
    );
    // Exactly at the limit is fine; one past is not.
    assert_eq!(lines(b"1234\n", 4), [some("1234"), Ok(None)]);
    assert_eq!(lines(b"12345\n", 4), [Err("too long 4".into()), Ok(None)]);
    // And at the end of the stream, with no newline.
    assert_eq!(lines(b"12345", 4), [Err("too long 4".into()), Ok(None)]);

    let e = LineError::TooLong { limit: 16 }.malformed().unwrap();
    assert_eq!(e.error.code, LINE_TOO_LONG);
}

#[test]
fn a_line_that_is_not_utf8_is_a_parse_error() {
    assert_eq!(
        lines(b"ok\n\xff\xfe\nnext\n", 64),
        [some("ok"), Err("not utf-8".into()), some("next"), Ok(None)]
    );
    let m = LineError::NotUtf8.malformed().unwrap();
    assert_eq!(m.error.code, PARSE_ERROR);
}

#[test]
fn a_broken_pipe_is_no_protocol_error() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("pipe"))
        }
    }
    let mut reader = LineReader::new(BufReader::new(Broken));
    let e = reader.next_line().unwrap_err();
    assert!(e.malformed().is_none(), "nothing to answer on a dead pipe");
}

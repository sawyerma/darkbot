//! Bounded parsing of provider responses.

use darkbot_runner::inclusion::response::{ProtocolFailure, ResponseBuffer, ResponseOutcome};

/// The id of the request every response here answers.
const ID: &str = "0x00000000000000000000000000000000000000000000000000000000000000aa";

/// A bundle hash: `0x` and 64 hex digits.
const HASH: &str = "0x1f2e3d4c5b6a79881f2e3d4c5b6a79881f2e3d4c5b6a79881f2e3d4c5b6a7988";

fn parse_with(capacity: usize, data: &[u8]) -> Option<ResponseOutcome> {
    let mut buffer = ResponseBuffer::new(capacity);
    let spare = buffer.spare();
    let count = data.len().min(spare.len());
    spare[..count].copy_from_slice(&data[..count]);
    buffer.advance(count, ID.as_bytes())
}

fn parse(data: &[u8]) -> Option<ResponseOutcome> {
    parse_with(4_096, data)
}

/// A body with the version and id of the request, then `members`.
fn answer(members: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":"{ID}",{members}}}"#)
}

/// The members of an acknowledgement.
fn acked() -> String {
    format!(r#""result":"{HASH}""#)
}

fn json(status_line: &str, body: &str) -> Vec<u8> {
    format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn ok(body: &str) -> Option<ResponseOutcome> {
    parse(&json("HTTP/1.1 200 OK", body))
}

fn protocol(failure: ProtocolFailure) -> Option<ResponseOutcome> {
    Some(ResponseOutcome::Protocol(failure))
}

#[test]
fn result_and_error_are_classified() {
    assert_eq!(HASH.len(), 66);
    assert_eq!(ok(&answer(&acked())), Some(ResponseOutcome::Ack));
    assert_eq!(
        ok(&answer(r#""error":{"code":-38000,"message":"refused"}"#)),
        Some(ResponseOutcome::Reject {
            code: Some(-38_000)
        })
    );
    // An error without an integer code is still a rejection.
    assert_eq!(
        ok(&answer(r#""error":{"message":"refused"}"#)),
        Some(ResponseOutcome::Reject { code: None })
    );
    assert_eq!(
        ok(&answer(r#""error":"refused""#)),
        Some(ResponseOutcome::Reject { code: None })
    );
    assert_eq!(
        ok(&answer(r#""error":{"code":1.5}"#)),
        Some(ResponseOutcome::Reject { code: None })
    );
    // Whitespace and member order do not matter.
    let reordered =
        format!("{{ \"result\" : \"{HASH}\" ,\n \"id\" : \"{ID}\" , \"jsonrpc\" : \"2.0\" }}");
    assert_eq!(ok(&reordered), Some(ResponseOutcome::Ack));
    // Upper-case hex digits are hex digits.
    let upper = format!("0x{}", HASH[2..].to_ascii_uppercase());
    let members = format!(r#""result":"{upper}""#);
    assert_eq!(ok(&answer(&members)), Some(ResponseOutcome::Ack));
}

#[test]
fn only_a_bundle_hash_is_an_acknowledgement() {
    let digits = &HASH[2..];
    for result in [
        // Short, empty, one digit less, one digit more.
        "0x01".to_string(),
        String::new(),
        format!("0x{}", &digits[..63]),
        format!("0x{digits}0"),
        // A character that is not a hex digit.
        format!("0x{}g", &digits[..63]),
        // Without the prefix, or with another one.
        digits.to_string(),
        format!("0X{digits}"),
        format!("00{digits}"),
    ] {
        let members = format!(r#""result":"{result}""#);
        assert_eq!(
            ok(&answer(&members)),
            protocol(ProtocolFailure::UnexpectedResult),
            "{result:?}"
        );
    }
    // Not a string at all.
    for result in ["null", "true", "1", "{}", "[]"] {
        let members = format!(r#""result":{result}"#);
        assert_eq!(
            ok(&answer(&members)),
            protocol(ProtocolFailure::UnexpectedResult),
            "{result}"
        );
    }
}

#[test]
fn only_a_response_to_this_request_counts() {
    // Another id, a numeric id, a null id: not this request.
    for id in [
        r#""id":"0x01""#,
        r#""id":"1""#,
        r#""id":170"#,
        r#""id":null"#,
    ] {
        let body = format!(r#"{{"jsonrpc":"2.0",{id},"result":"{HASH}"}}"#);
        assert_eq!(ok(&body), protocol(ProtocolFailure::WrongId), "{id}");
    }
    let missing = format!(r#"{{"jsonrpc":"2.0","result":"{HASH}"}}"#);
    assert_eq!(ok(&missing), protocol(ProtocolFailure::WrongId));
    // Another version, or none.
    let other_version = format!(r#"{{"jsonrpc":"1.0","id":"{ID}","result":"{HASH}"}}"#);
    assert_eq!(ok(&other_version), protocol(ProtocolFailure::WrongVersion));
    let no_version = format!(r#"{{"id":"{ID}","result":"{HASH}"}}"#);
    assert_eq!(ok(&no_version), protocol(ProtocolFailure::WrongVersion));
    // A rejection of another request is no rejection of this one either.
    let foreign_reject = r#"{"jsonrpc":"2.0","id":"0x01","error":{"code":-1}}"#;
    assert_eq!(ok(foreign_reject), protocol(ProtocolFailure::WrongId));
    // Two ids are malformed.
    let twice = format!(r#"{{"jsonrpc":"2.0","id":"{ID}","id":"{ID}","result":"{HASH}"}}"#);
    assert_eq!(ok(&twice), protocol(ProtocolFailure::MalformedJson));
}

#[test]
fn a_body_that_is_not_a_plain_answer_is_a_protocol_failure() {
    assert_eq!(
        ok(&answer(r#""x":1"#)),
        protocol(ProtocolFailure::NeitherResultNorError)
    );
    let both = format!(r#"{},"error":{{"code":1}}"#, acked());
    assert_eq!(
        ok(&answer(&both)),
        protocol(ProtocolFailure::BothResultAndError)
    );
    for malformed in [
        answer(&format!("{},", acked())),
        format!("{} x", answer(&acked())),
        answer(&format!("{},{}", acked(), acked())),
        answer(r#""result":"\x""#),
        answer(&format!(r#"{},"n":01"#, acked())),
        r#"["result"]"#.to_string(),
        String::new(),
    ] {
        assert_eq!(
            ok(&malformed),
            protocol(ProtocolFailure::MalformedJson),
            "{malformed}"
        );
    }
    // Nesting is bounded.
    let deep = answer(&format!(
        r#"{},"x":{}{}"#,
        acked(),
        "[".repeat(40),
        "]".repeat(40)
    ));
    assert_eq!(ok(&deep), protocol(ProtocolFailure::MalformedJson));
}

#[test]
fn http_status_classification_ignores_complete_body_contents() {
    // A 429 is bound to the request by the connection. Its complete HTTP
    // body is consumed for framing but its contents are not interpreted.
    assert_eq!(
        parse(&json("HTTP/1.1 429 Too Many Requests", "{}")),
        Some(ResponseOutcome::RateLimited)
    );
    assert_eq!(
        parse(&json("HTTP/1.1 429 Too Many Requests", "not json")),
        Some(ResponseOutcome::RateLimited)
    );
    // Any other status than 200 and 429 moves nothing, even with an
    // acknowledgement in the body.
    assert_eq!(
        parse(&json("HTTP/1.1 401 Unauthorized", &answer(&acked()))),
        protocol(ProtocolFailure::HttpStatus(401))
    );
    assert_eq!(
        parse(&json("HTTP/1.1 502 Bad Gateway", "")),
        protocol(ProtocolFailure::HttpStatus(502))
    );
}

#[test]
fn the_body_must_be_framed_exactly_once() {
    let body = &answer(&acked());
    // Chunked.
    let chunked = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
         {:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n",
        5,
        &body[..5],
        body.len() - 5,
        &body[5..]
    );
    assert_eq!(parse(chunked.as_bytes()), Some(ResponseOutcome::Ack));
    // Header names are case-insensitive.
    let lower = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(parse(lower.as_bytes()), Some(ResponseOutcome::Ack));

    // No framing: the end cannot be known.
    let unframed = format!("HTTP/1.1 200 OK\r\n\r\n{body}");
    assert_eq!(
        parse(unframed.as_bytes()),
        protocol(ProtocolFailure::UnframedBody)
    );
    // Both framings, two lengths, an unknown coding, bytes beyond the
    // length, a broken chunk, another protocol, a bad status, a bad
    // header: refused.
    for refused in [
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nTransfer-Encoding: chunked\r\n\r\n{body}",
            body.len()
        ),
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {0}\r\nContent-Length: {0}\r\n\r\n{body}",
            body.len()
        ),
        format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n{body}"),
        format!("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{body}"),
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n{}\r\n0\r\n\r\n".to_string(),
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}XX0\r\n\r\n".to_string(),
        "HTTP/2 200\r\nContent-Length: 2\r\n\r\n{}".to_string(),
        "HTTP/1.1 2x0 OK\r\nContent-Length: 2\r\n\r\n{}".to_string(),
        "HTTP/1.1 200 OK\r\nno colon\r\nContent-Length: 2\r\n\r\n{}".to_string(),
    ] {
        assert_eq!(
            parse(refused.as_bytes()),
            protocol(ProtocolFailure::MalformedHttp),
            "{refused:?}"
        );
    }
}

#[test]
fn an_incomplete_response_waits_and_an_oversize_one_fails() {
    let full = json("HTTP/1.1 200 OK", &answer(&acked()));
    // Every proper prefix is incomplete.
    for cut in [0, 10, full.len() / 2, full.len() - 1] {
        assert_eq!(parse(&full[..cut]), None, "{cut}");
    }
    // Arriving in pieces, it completes.
    let mut buffer = ResponseBuffer::new(4_096);
    let mut outcome = None;
    for piece in full.chunks(7) {
        let spare = buffer.spare();
        spare[..piece.len()].copy_from_slice(piece);
        outcome = buffer.advance(piece.len(), ID.as_bytes());
    }
    assert_eq!(outcome, Some(ResponseOutcome::Ack));
    // Reset forgets it.
    buffer.reset();
    assert_eq!(buffer.advance(0, ID.as_bytes()), None);

    // Storage full and still incomplete: oversize, never grown.
    assert_eq!(
        parse_with(full.len() - 1, &full),
        protocol(ProtocolFailure::Oversize)
    );
    assert_eq!(parse_with(full.len(), &full), Some(ResponseOutcome::Ack));
}

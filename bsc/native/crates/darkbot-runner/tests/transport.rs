//! The stream transport and the monotonic clock.

use std::io::Cursor;

use darkbot_runner::inclusion::transport::{
    Clock, ConnectionState, DiagnosticStreamTransport, MonotonicClock, Transport,
};

#[test]
fn a_diagnostic_stream_transport_writes_through_and_only_records_its_state() {
    let mut transport = DiagnosticStreamTransport::ready(Cursor::new(Vec::new()));
    assert_eq!(transport.state(), ConnectionState::Ready);
    assert_eq!(transport.write(b"abc").unwrap(), 3);
    transport.flush().unwrap();
    assert_eq!(transport.stream().get_ref().as_slice(), b"abc");
    // Marking changes the recorded state and nothing else: no reconnect.
    transport.mark(ConnectionState::Stale);
    assert_eq!(transport.state(), ConnectionState::Stale);
    assert_eq!(transport.stream().get_ref().as_slice(), b"abc");
}

#[test]
fn the_monotonic_clock_does_not_go_back() {
    let clock = MonotonicClock::start(9);
    assert_eq!(clock.id(), 9);
    let mut last = clock.now_ns();
    for _ in 0..1_000 {
        let now = clock.now_ns();
        assert!(now >= last);
        last = now;
    }
}

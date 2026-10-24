//! Rust↔TS golden bytes of the shared control protocol (Wire V1.3 §8/§9): u64/i64 limits,
//! optional fields, heartbeat watermarks, commands and the overview aggregate
//! with an i128 signed decimal. frontend/src/core/realtime/control-golden.test.ts decodes the
//! same bytes and re-encodes them byte for byte.
use darkbot_gateway::{
    proto::{
        expected_version, server_message, CommandPhase, CommandRejectReason, CommandRequest,
        CommandResponse, CommandStatus, ExpectedVersion, FinanceQuality, FinanceSummary,
        GlobalFinance, Heartbeat, OverviewAggregates, ProjectId, ProjectJob, ProjectSummary,
        Readiness, ServerLimits, ServerMessage, SignedDecimal, SourceState, StateFull,
        StateQuality, StateWatermark, TopicId, Welcome, WelcomeStatus,
    },
    PROTOCOL_MAJOR, PROTOCOL_MINOR,
};
use prost::Message;
use std::{fs, path::PathBuf};

const REQUEST: &str = "123e4567-e89b-12d3-a456-426614174000";

fn check(name: &str, bytes: Vec<u8>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../protocol/golden/control");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>() + "\n";
    let path = dir.join(format!("{name}.hex"));
    if std::env::var_os("UPDATE_CONTROL_GOLDENS").is_some() {
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, &hex).unwrap();
    } else {
        assert_eq!(
            fs::read_to_string(&path).unwrap_or_default(),
            hex,
            "{name}: regenerate with UPDATE_CONTROL_GOLDENS=1 after a reviewed wire change"
        );
    }
}
fn server(m: server_message::Msg) -> Vec<u8> {
    ServerMessage { msg: Some(m) }.encode_to_vec()
}

#[test]
fn control_protocol_golden_bytes() {
    check(
        "welcome",
        server(server_message::Msg::Welcome(Welcome {
            status: WelcomeStatus::Accepted as i32,
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            schema_id: "golden-schema".into(),
            connection_id: "golden".into(),
            server_time_unix_ns: i64::MAX,
            limits: Some(ServerLimits {
                max_state_full_bytes: 262_144,
                heartbeat_interval_ms: 15_000,
                stale_after_missed_heartbeats: 2,
                unsubscribe_grace_ms: 2_000,
                max_catchup_retries: 3,
            }),
            topics: vec![],
            reject_reason: String::new(),
        })),
    );
    check(
        "state_full",
        server(server_message::Msg::StateFull(StateFull {
            topic: TopicId::BscStatus as i32,
            generation: u64::MAX,
            seq: u64::MAX,
            source_timestamp_unix_ns: i64::MIN,
            quality: StateQuality::SourceDetached as i32,
            source_id: "golden".into(),
            payload: vec![0, 1, 255].into(),
        })),
    );
    check(
        "heartbeat",
        server(server_message::Msg::Heartbeat(Heartbeat {
            heartbeat_id: u64::MAX,
            server_time_unix_ns: 1,
            state_watermarks: vec![StateWatermark {
                topic: TopicId::Project0Status as i32,
                generation: u64::MAX,
                highest_seq_sent: 7,
            }],
        })),
    );
    check(
        "command_request",
        CommandRequest {
            project: ProjectId::Bsc as i32,
            request_id: REQUEST.into(),
            action: "bsc.candidate.select".into(),
            resource_id: "SELF_B".into(),
            expected: Some(ExpectedVersion {
                version: Some(expected_version::Version::ProfileVersion(u64::MAX)),
            }),
            params: Default::default(),
            issued_at_unix_ns: i64::MIN,
        }
        .encode_to_vec(),
    );
    check(
        "command_response",
        CommandResponse {
            status: CommandStatus::Accepted as i32,
            command_id: REQUEST.into(),
            request_id: REQUEST.into(),
            reject_reason: CommandRejectReason::Unspecified as i32,
            detail: "recorded".into(),
            persisted: None,
            phase: CommandPhase::PendingReconcile as i32,
        }
        .encode_to_vec(),
    );
    check(
        "overview_aggregates",
        OverviewAggregates {
            projects: vec![ProjectSummary {
                project: ProjectId::Bsc as i32,
                label: "BSC".into(),
                domain: "BNB Smart Chain".into(),
                source: SourceState::Connected as i32,
                readiness: Readiness::Unknown as i32,
                live_authorized: Some(false),
                active_bots: None,
                active_jobs: Some(0),
                active_captures: None,
                warnings: vec!["Pre-Live: BLOCKED".into()],
                rpc_latency: None,
                finance: Some(FinanceSummary {
                    finance_snapshot_id: "golden".into(),
                    quality: FinanceQuality::Finalized as i32,
                    realized_net_pnl_usd_micro_signed: Some(SignedDecimal {
                        value: i128::MIN.to_string(),
                    }),
                    total_capital_usd_micro: Some(u64::MAX),
                    as_of_unix_ns: 1,
                }),
                source_timestamp_unix_ns: 1,
                source_id: "bsc-status-reporter".into(),
                jobs: vec![ProjectJob {
                    job: "readiness".into(),
                    state: "STOPPED".into(),
                    launch_id: "launch".into(),
                    result_code: "BLOCKED".into(),
                    controller_available: Some(true),
                }],
            }],
            global: Some(GlobalFinance {
                available: false,
                unavailable_reason: "no finalized cross-project valuation".into(),
                net_pnl_usd_micro_signed: None,
                total_capital_usd_micro: None,
            }),
            projects_connected: 1,
            computed_at_unix_ns: 1,
        }
        .encode_to_vec(),
    );
}

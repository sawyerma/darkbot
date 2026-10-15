//! Q5B: promotion of the accepted Q5A TLS evidence into an opaque
//! production transport qualification.
//!
//! The accepted evidence is deliberately narrow. It measured one frozen
//! TLS implementation on macOS/aarch64, with one policy and one concrete
//! network/TLS profile for each builder. This module therefore fails closed
//! on every field of [`TlsProfile`] that Q5A recorded. A different peer,
//! cipher, endpoint, policy, platform or implementation needs a new
//! qualification generation; it is not silently inherited.
//!
//! Provider semantics are not promoted here. The capability records remain
//! unchanged and their ordering/atomicity/same-block/features stay
//! unqualified.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use darkbot_bsc_trading::intent::AdapterId;
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterKind, AdapterTable, HandoffBoundary,
};
use rustls::{CipherSuite, ProtocolVersion};

use super::{
    BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT, HttpVersion,
    TLS_TRANSPORT_IMPLEMENTATION, TlsConnectionOwner, TlsPolicy, TlsProfile,
};
use crate::submission::transport::{
    ConnectionState, QualifiedTransport, TransportBinding, TransportQualification,
    bind_accepted_tls,
};

pub const Q5A_TLS_EVIDENCE_PATH: &str = "bsc/native/evidence/q5a/q5a-both-20261005T203919Z.json";
pub const Q5A_TLS_EVIDENCE_SHA256: &str =
    "e2c537765f539f7551d89613c337d2079820731b77a531b7306a14fe4c845849";
pub const Q5A_TLS_QUALIFICATION_GENERATION: u64 = 1;

/// Opaque proof that the current owner/profile passed the accepted Q5A
/// checks in this module. The type is crate-visible so the transport module
/// can accept it, but its field is private here: no other module can
/// construct one.
pub(crate) struct AcceptedTlsEvidence {
    _sealed: (),
}

pub const Q5A_TLS_POLICY: TlsPolicy = TlsPolicy {
    connect_timeout: Duration::from_secs(5),
    handshake_timeout: Duration::from_secs(5),
    max_idle: Duration::from_secs(10),
    response_timeout: Duration::from_secs(5),
    max_plaintext_request_len: 64 * 1024,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsQualificationError {
    UnboundAdapter,
    UnsupportedAdapter,
    HandoffBoundaryMismatch,
    NoConnection,
    ConnectionNotReady(ConnectionState),
    ImplementationMismatch,
    HostMismatch,
    PortMismatch,
    PolicyMismatch,
    TargetMismatch,
    ConnectionGenerationInvalid,
    PeerMismatch,
    TlsVersionMismatch,
    CipherSuiteMismatch,
    HttpMismatch,
    AlpnMismatch,
}

impl fmt::Display for TlsQualificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnboundAdapter => f.write_str("tls qualification: adapter is not bound"),
            Self::UnsupportedAdapter => {
                f.write_str("tls qualification: adapter kind was not qualified by Q5A")
            }
            Self::HandoffBoundaryMismatch => {
                f.write_str("tls qualification: capability handoff boundary differs")
            }
            Self::NoConnection => f.write_str("tls qualification: no established connection"),
            Self::ConnectionNotReady(state) => {
                write!(f, "tls qualification: connection is not ready ({state:?})")
            }
            Self::ImplementationMismatch => {
                f.write_str("tls qualification: transport implementation differs")
            }
            Self::HostMismatch => f.write_str("tls qualification: endpoint host differs"),
            Self::PortMismatch => f.write_str("tls qualification: endpoint port differs"),
            Self::PolicyMismatch => f.write_str("tls qualification: TLS policy differs"),
            Self::TargetMismatch => f.write_str("tls qualification: target platform differs"),
            Self::ConnectionGenerationInvalid => {
                f.write_str("tls qualification: connection generation is invalid")
            }
            Self::PeerMismatch => f.write_str("tls qualification: connected peer differs"),
            Self::TlsVersionMismatch => {
                f.write_str("tls qualification: negotiated TLS version differs")
            }
            Self::CipherSuiteMismatch => {
                f.write_str("tls qualification: negotiated cipher suite differs")
            }
            Self::HttpMismatch => f.write_str("tls qualification: HTTP version differs"),
            Self::AlpnMismatch => f.write_str("tls qualification: ALPN selection differs"),
        }
    }
}

impl std::error::Error for TlsQualificationError {}

#[derive(Clone, Copy)]
struct AcceptedProfile {
    host: &'static str,
    peer: Ipv4Addr,
    cipher_suite: CipherSuite,
}

fn accepted_profile(kind: AdapterKind) -> Result<AcceptedProfile, TlsQualificationError> {
    match kind {
        AdapterKind::Club48Builder => Ok(AcceptedProfile {
            host: CLUB48_DOCUMENTED_HOST,
            peer: Ipv4Addr::new(135, 125, 142, 202),
            cipher_suite: CipherSuite::TLS13_AES_256_GCM_SHA384,
        }),
        AdapterKind::BlockRazorBuilder => Ok(AcceptedProfile {
            host: BLOCKRAZOR_DOCUMENTED_HOST,
            peer: Ipv4Addr::new(99, 86, 253, 99),
            cipher_suite: CipherSuite::TLS13_AES_128_GCM_SHA256,
        }),
        AdapterKind::LocalGeth | AdapterKind::BlockRazorRpc => {
            Err(TlsQualificationError::UnsupportedAdapter)
        }
    }
}

fn binding(
    table: &AdapterTable,
    adapter: AdapterId,
) -> Result<(AdapterKind, HandoffBoundary), TlsQualificationError> {
    let record = table
        .record(adapter)
        .ok_or(TlsQualificationError::UnboundAdapter)?;
    let kind = record.kind();
    let _ = accepted_profile(kind)?;
    let boundary = record.handoff_boundary();
    if boundary != HandoffBoundary::TlsUserspaceWriteFlushReturned {
        return Err(TlsQualificationError::HandoffBoundaryMismatch);
    }
    Ok((kind, boundary))
}

fn validate_profile(kind: AdapterKind, profile: &TlsProfile) -> Result<(), TlsQualificationError> {
    let accepted = accepted_profile(kind)?;
    if profile.implementation != TLS_TRANSPORT_IMPLEMENTATION {
        return Err(TlsQualificationError::ImplementationMismatch);
    }
    if profile.host != accepted.host {
        return Err(TlsQualificationError::HostMismatch);
    }
    if profile.port != HTTPS_PORT {
        return Err(TlsQualificationError::PortMismatch);
    }
    if profile.policy != Q5A_TLS_POLICY {
        return Err(TlsQualificationError::PolicyMismatch);
    }
    if profile.target != ("macos", "aarch64") {
        return Err(TlsQualificationError::TargetMismatch);
    }
    let connection = profile.connection;
    if connection.generation == 0 {
        return Err(TlsQualificationError::ConnectionGenerationInvalid);
    }
    if connection.peer.ip() != IpAddr::V4(accepted.peer) || connection.peer.port() != HTTPS_PORT {
        return Err(TlsQualificationError::PeerMismatch);
    }
    if connection.tls_version != ProtocolVersion::TLSv1_3 {
        return Err(TlsQualificationError::TlsVersionMismatch);
    }
    if connection.cipher_suite != accepted.cipher_suite {
        return Err(TlsQualificationError::CipherSuiteMismatch);
    }
    if profile.http != HttpVersion::Http11 || connection.http != HttpVersion::Http11 {
        return Err(TlsQualificationError::HttpMismatch);
    }
    if !connection.alpn_selected {
        return Err(TlsQualificationError::AlpnMismatch);
    }
    Ok(())
}

pub struct QualifiedTlsTransport<'a> {
    transport: QualifiedTransport<'a>,
    qualification: TransportQualification,
}

impl<'a> QualifiedTlsTransport<'a> {
    pub fn qualification(&self) -> TransportQualification {
        self.qualification
    }

    pub fn binding(&self) -> TransportBinding {
        self.transport.binding()
    }

    pub fn into_parts(self) -> (QualifiedTransport<'a>, TransportQualification) {
        (self.transport, self.qualification)
    }
}

pub fn qualify_transport<'a>(
    owner: &'a mut TlsConnectionOwner,
    table: &AdapterTable,
    adapter: AdapterId,
) -> Result<QualifiedTlsTransport<'a>, TlsQualificationError> {
    let (kind, boundary) = binding(table, adapter)?;
    let state = owner.state();
    if state != ConnectionState::Ready {
        return Err(TlsQualificationError::ConnectionNotReady(state));
    }
    let profile = owner.profile().ok_or(TlsQualificationError::NoConnection)?;
    validate_profile(kind, &profile)?;

    let evidence = AcceptedTlsEvidence { _sealed: () };
    let (transport, qualification) = bind_accepted_tls(
        owner.transport(),
        adapter,
        kind,
        table.generation(),
        boundary,
        evidence,
        Q5A_TLS_QUALIFICATION_GENERATION,
    );
    Ok(QualifiedTlsTransport {
        transport,
        qualification,
    })
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use darkbot_bsc_trading::submission_policy::capability::{AdapterBinding, CapabilityId};
    use rustls::{CipherSuite, ProtocolVersion};

    use super::*;
    use crate::submission::tls::ConnectionFacts;

    fn adapter(index: u8) -> AdapterId {
        AdapterId::new(index).unwrap()
    }

    fn table(capability: CapabilityId) -> AdapterTable {
        AdapterTable::new(
            56,
            9,
            &[AdapterBinding {
                adapter: adapter(0),
                capability,
            }],
        )
        .unwrap()
    }

    fn profile(kind: AdapterKind) -> TlsProfile {
        let accepted = accepted_profile(kind).unwrap();
        TlsProfile {
            implementation: TLS_TRANSPORT_IMPLEMENTATION,
            host: accepted.host.to_owned(),
            port: HTTPS_PORT,
            connection: ConnectionFacts {
                generation: 1,
                peer: SocketAddr::new(IpAddr::V4(accepted.peer), HTTPS_PORT),
                tls_version: ProtocolVersion::TLSv1_3,
                cipher_suite: accepted.cipher_suite,
                http: HttpVersion::Http11,
                alpn_selected: true,
            },
            http: HttpVersion::Http11,
            policy: Q5A_TLS_POLICY,
            target: ("macos", "aarch64"),
        }
    }

    #[test]
    fn q5a_identity_is_pinned_and_generation_is_nonzero() {
        assert_eq!(
            Q5A_TLS_EVIDENCE_SHA256,
            "e2c537765f539f7551d89613c337d2079820731b77a531b7306a14fe4c845849"
        );
        assert_eq!(
            Q5A_TLS_EVIDENCE_PATH,
            "bsc/native/evidence/q5a/q5a-both-20261005T203919Z.json"
        );
        assert_ne!(Q5A_TLS_QUALIFICATION_GENERATION, 0);
    }

    #[test]
    fn both_measured_provider_profiles_pass() {
        assert_eq!(
            validate_profile(
                AdapterKind::Club48Builder,
                &profile(AdapterKind::Club48Builder)
            ),
            Ok(())
        );
        assert_eq!(
            validate_profile(
                AdapterKind::BlockRazorBuilder,
                &profile(AdapterKind::BlockRazorBuilder)
            ),
            Ok(())
        );
    }

    #[test]
    fn only_the_two_direct_builder_capabilities_can_inherit_q5a() {
        assert_eq!(
            binding(&table(CapabilityId::Club48BuilderV1), adapter(0)),
            Ok((
                AdapterKind::Club48Builder,
                HandoffBoundary::TlsUserspaceWriteFlushReturned
            ))
        );
        assert_eq!(
            binding(&table(CapabilityId::BlockRazorBuilderV1), adapter(0)),
            Ok((
                AdapterKind::BlockRazorBuilder,
                HandoffBoundary::TlsUserspaceWriteFlushReturned
            ))
        );
        assert_eq!(
            binding(&table(CapabilityId::LocalGethV1), adapter(0)),
            Err(TlsQualificationError::UnsupportedAdapter)
        );
        assert_eq!(
            binding(&table(CapabilityId::BlockRazorRpcV1), adapter(0)),
            Err(TlsQualificationError::UnsupportedAdapter)
        );
        assert_eq!(
            binding(&table(CapabilityId::Club48BuilderV1), adapter(1)),
            Err(TlsQualificationError::UnboundAdapter)
        );
    }

    #[test]
    fn implementation_endpoint_policy_and_target_mismatches_fail_closed() {
        let mut p = profile(AdapterKind::Club48Builder);
        p.implementation = "other";
        assert_eq!(
            validate_profile(AdapterKind::Club48Builder, &p),
            Err(TlsQualificationError::ImplementationMismatch)
        );

        let mut p = profile(AdapterKind::Club48Builder);
        p.host = BLOCKRAZOR_DOCUMENTED_HOST.to_owned();
        assert_eq!(
            validate_profile(AdapterKind::Club48Builder, &p),
            Err(TlsQualificationError::HostMismatch)
        );

        let mut p = profile(AdapterKind::Club48Builder);
        p.port = HTTPS_PORT + 1;
        assert_eq!(
            validate_profile(AdapterKind::Club48Builder, &p),
            Err(TlsQualificationError::PortMismatch)
        );

        let mut p = profile(AdapterKind::Club48Builder);
        p.policy.max_idle = Duration::from_secs(11);
        assert_eq!(
            validate_profile(AdapterKind::Club48Builder, &p),
            Err(TlsQualificationError::PolicyMismatch)
        );

        let mut p = profile(AdapterKind::Club48Builder);
        p.target = ("linux", "aarch64");
        assert_eq!(
            validate_profile(AdapterKind::Club48Builder, &p),
            Err(TlsQualificationError::TargetMismatch)
        );
    }

    #[test]
    fn peer_generation_tls_cipher_and_alpn_mismatches_fail_closed() {
        let mut p = profile(AdapterKind::BlockRazorBuilder);
        p.connection.generation = 0;
        assert_eq!(
            validate_profile(AdapterKind::BlockRazorBuilder, &p),
            Err(TlsQualificationError::ConnectionGenerationInvalid)
        );

        let mut p = profile(AdapterKind::BlockRazorBuilder);
        p.connection.peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), HTTPS_PORT);
        assert_eq!(
            validate_profile(AdapterKind::BlockRazorBuilder, &p),
            Err(TlsQualificationError::PeerMismatch)
        );

        let mut p = profile(AdapterKind::BlockRazorBuilder);
        p.connection.tls_version = ProtocolVersion::TLSv1_2;
        assert_eq!(
            validate_profile(AdapterKind::BlockRazorBuilder, &p),
            Err(TlsQualificationError::TlsVersionMismatch)
        );

        let mut p = profile(AdapterKind::BlockRazorBuilder);
        p.connection.cipher_suite = CipherSuite::TLS13_AES_256_GCM_SHA384;
        assert_eq!(
            validate_profile(AdapterKind::BlockRazorBuilder, &p),
            Err(TlsQualificationError::CipherSuiteMismatch)
        );

        let mut p = profile(AdapterKind::BlockRazorBuilder);
        p.connection.alpn_selected = false;
        assert_eq!(
            validate_profile(AdapterKind::BlockRazorBuilder, &p),
            Err(TlsQualificationError::AlpnMismatch)
        );
    }
}

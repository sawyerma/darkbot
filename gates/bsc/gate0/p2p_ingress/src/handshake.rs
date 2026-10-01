// Pinned from bnb-chain/reth-bsc @ 98ef6c5b2d8b191f9b10bf1406dce341475c30ce.
// Gate-0 is deliberately non-EVN, so disable_peer_tx_broadcast is always false.
use crate::upgrade_status::{UpgradeStatus, UpgradeStatusExtension};
use alloy_rlp::Decodable;
use futures::SinkExt;
use reth_eth_wire::{
    errors::{EthHandshakeError, EthStreamError},
    handshake::{EthRlpxHandshake, EthereumEthHandshake, UnauthEth},
    UnifiedStatus,
};
use reth_eth_wire_types::{DisconnectReason, EthVersion};
use reth_ethereum_forks::ForkFilter;
use std::{future::Future, pin::Pin};
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;

#[derive(Debug, Default)]
pub struct BscHandshake;

impl BscHandshake {
    async fn upgrade_status(
        unauth: &mut dyn UnauthEth,
        negotiated_status: UnifiedStatus,
    ) -> Result<UnifiedStatus, EthStreamError> {
        if negotiated_status.version > EthVersion::Eth66 {
            let upgrade_msg = UpgradeStatus {
                extension: UpgradeStatusExtension {
                    disable_peer_tx_broadcast: false,
                },
            };
            unauth.start_send_unpin(upgrade_msg.into_rlpx())?;

            let their_msg = match unauth.next().await {
                Some(Ok(msg)) => msg,
                Some(Err(e)) => return Err(EthStreamError::from(e)),
                None => {
                    unauth.disconnect(DisconnectReason::DisconnectRequested).await?;
                    return Err(EthStreamError::EthHandshakeError(EthHandshakeError::NoResponse));
                }
            };

            if UpgradeStatus::decode(&mut their_msg.as_ref()).is_err() {
                unauth.disconnect(DisconnectReason::DisconnectRequested).await?;
                return Err(EthStreamError::EthHandshakeError(EthHandshakeError::NoResponse));
            }
        }
        Ok(negotiated_status)
    }
}

impl EthRlpxHandshake for BscHandshake {
    fn handshake<'a>(
        &'a self,
        unauth: &'a mut dyn UnauthEth,
        status: UnifiedStatus,
        fork_filter: ForkFilter,
        timeout_limit: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedStatus, EthStreamError>> + 'a + Send>> {
        Box::pin(async move {
            let fut = async {
                let negotiated =
                    EthereumEthHandshake(unauth).eth_handshake(status, fork_filter).await?;
                Self::upgrade_status(unauth, negotiated).await
            };
            timeout(timeout_limit, fut)
                .await
                .map_err(|_| EthStreamError::StreamTimeout)?
        })
    }
}

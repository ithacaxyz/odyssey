//! Helper that delays resolving the payload

use futures::{Stream, StreamExt};
use jsonrpsee::{
    core::traits::ToRpcParams,
    types::{error::INVALID_PARAMS_CODE, ErrorObject, Params},
    MethodsError, RpcModule,
};
use parking_lot::Mutex;
use reth_chain_state::CanonStateNotification;
use serde::de::Error;
use serde_json::value::RawValue;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Delay into the slot
pub const MAX_DELAY_INTO_SLOT: Duration = Duration::from_millis(500);

/// The getpayload fn we want to delay
pub const GET_PAYLOAD_V3: &str = "engine_getPayloadV3";

/// A helper that tracks the block clock timestamp and can delay resolving the payload to give the
/// payload builder more time to build a block.
#[derive(Debug, Clone)]
pub struct DelayedResolver {
    inner: Arc<DelayedResolverInner>,
}

impl DelayedResolver {
    /// Creates a new instance with the engine module and the duration we should target
    pub fn new(engine_module: RpcModule<()>, max_delay_into_slot: Duration) -> Self {
        Self {
            inner: Arc::new(DelayedResolverInner {
                last_block_time: Mutex::new(Instant::now()),
                engine_module,
                max_delay_into_slot,
            }),
        }
    }

    /// Listen for new blocks and track the local timestamp.
    pub fn spawn<St>(self, mut st: St)
    where
        St: Stream<Item = CanonStateNotification> + Send + Unpin + 'static,
    {
        tokio::task::spawn(async move {
            while st.next().await.is_some() {
                *self.inner.last_block_time.lock() = Instant::now();
            }
        });
    }

    async fn call(&self, params: Params<'static>) -> Result<serde_json::Value, MethodsError> {
        let last = *self.inner.last_block_time.lock();
        let now = Instant::now();
        // how far we're into the slot
        let offset = now.duration_since(last);

        if offset < self.inner.max_delay_into_slot {
            // if we received the request before the max delay exceeded we can delay the request to
            // give the payload builder more time to build the payload.
            let delay = self.inner.max_delay_into_slot.saturating_sub(offset);
            tokio::time::sleep(delay).await;
        }

        let params = params
            .as_str()
            .ok_or_else(|| MethodsError::Parse(serde_json::Error::missing_field("payload id")))?;

        self.inner.engine_module.call(GET_PAYLOAD_V3, PayloadParam(params.to_string())).await
    }

    /// Converts this type into a new [`RpcModule`] that delegates the get payload call.
    /// 
    /// # Errors
    /// Returns error if failed to register the RPC method.
    pub fn into_rpc_module(self) -> Result<RpcModule<()>, jsonrpsee::core::Error> {
        let mut module = RpcModule::new(());
        module.register_async_method(GET_PAYLOAD_V3, move |params, _ctx, _| {
            let value = self.clone();
            async move {
                value.call(params).await.map_err(|err| match err {
                    MethodsError::JsonRpc(err) => err,
                    err => ErrorObject::owned(
                        INVALID_PARAMS_CODE,
                        format!("invalid payload call: {:?}", err),
                        None::<()>,
                    ),
                })
            }
        })?;

        Ok(module)
    }
}

#[derive(Debug)]
struct DelayedResolverInner {
    /// Tracks the time when the last block was emitted
    last_block_time: Mutex<Instant>,
    engine_module: RpcModule<()>,
    /// By how much we want to delay getPayload into the slot
    max_delay_into_slot: Duration,
}

struct PayloadParam(String);

impl ToRpcParams for PayloadParam {
    fn to_rpc_params(self) -> Result<Option<Box<RawValue>>, serde_json::Error> {
        RawValue::from_string(self.0).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_rpc_types::engine::PayloadId;

    /// Mocked payload object
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
    struct Payload {
        attributes: serde_json::Value,
        header: serde_json::Value,
    }

    #[tokio::test]
    async fn test_delayed_forward() -> Result<(), Box<dyn std::error::Error>> {
        use jsonrpsee::{core::RpcResult, RpcModule};

        let mut module = RpcModule::new(());
        module
            .register_method::<RpcResult<Payload>, _>(GET_PAYLOAD_V3, |params, _, _| {
                params.one::<PayloadId>()?;
                Ok(Payload::default())
            })?;

        let id = PayloadId::default();

        let _echo: Payload = module.call(GET_PAYLOAD_V3, [id]).await?;

        let delayer = DelayedResolver::new(module, MAX_DELAY_INTO_SLOT).into_rpc_module()?;
        let _echo: Payload = delayer.call(GET_PAYLOAD_V3, [id]).await?;
        
        Ok(())
    }
}

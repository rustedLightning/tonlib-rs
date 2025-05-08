use std::sync::Arc;

use async_trait::async_trait;
use tonlib_core::TonAddress;

use super::TonContractError;
use crate::client::TonConnection;
use crate::contract::TonContractFactory;
use crate::tl::{InternalTransactionId, RawFullAccountState};
use crate::types::{TonMethodId, TvmStackEntry, TvmSuccess};

pub struct LoadedSmcState {
    pub conn: TonConnection,
    pub id: i64,
}

#[async_trait]
pub trait TonContractInterface {
    fn factory(&self) -> &TonContractFactory;

    fn address(&self) -> &TonAddress;

    async fn get_account_state(&self) -> Result<Arc<RawFullAccountState>, TonContractError>;

    async fn get_account_state_by_transaction(
        &self,
        tx_id: &InternalTransactionId,
    ) -> Result<RawFullAccountState, TonContractError> {
        self.factory()
            .get_account_state_by_transaction(self.address(), tx_id)
            .await
    }

    async fn run_get_method<M, S>(
        &self,
        method: M,
        stack: S,
    ) -> Result<TvmSuccess, TonContractError>
    where
        M: Into<TonMethodId> + Send + Copy,
        S: AsRef<[TvmStackEntry]> + Send;
}

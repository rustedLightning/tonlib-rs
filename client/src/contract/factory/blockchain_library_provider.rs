use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use moka::future::Cache;
use tonlib_core::cell::{ArcCell, BagOfCells};
use tonlib_core::library_helper::{ContractLibraryDict, LibraryHelper, TonLibraryError};
use tonlib_core::TonHash;

use super::LibraryProvider;
use crate::client::{TonClient, TonClientInterface};
use crate::tl::TonLibraryId;

#[derive(Clone, Copy)]
pub struct LibraryCacheParams {
    capacity: u64,
    time_to_live: Duration,
}

impl Default for LibraryCacheParams {
    fn default() -> Self {
        const DEFAULT_LIBRARY_CACHE_CAPACITY: u64 = 300;
        const DEFAULT_LIBRARY_CACHE_TIME_TO_LIVE: Duration = Duration::from_secs(60 * 60);
        Self {
            capacity: DEFAULT_LIBRARY_CACHE_CAPACITY,
            time_to_live: DEFAULT_LIBRARY_CACHE_TIME_TO_LIVE,
        }
    }
}

#[derive(Clone)]
pub struct BlockchainLibraryProvider {
    inner: Arc<Inner>,
}

struct Inner {
    client: TonClient,
    cache: Cache<TonHash, ArcCell>,
}

#[async_trait]
impl LibraryProvider for BlockchainLibraryProvider {
    async fn get_libs(
        &self,
        cells: &[ArcCell],
        mc_seqno: Option<i32>,
    ) -> Result<ContractLibraryDict, TonLibraryError> {
        if mc_seqno.is_some() {
            return Err(TonLibraryError::SeqnoNotSupported);
        }

        let lib_ids = LibraryHelper::extract_library_hashes(cells)?;
        let libs = self.inner.get_libs(lib_ids).await?;
        LibraryHelper::store_to_dict(libs)
    }
}

impl BlockchainLibraryProvider {
    const MAX_LIBS_REQUESTED: usize = 255;

    pub fn new(client: &TonClient, cache_params: Option<LibraryCacheParams>) -> Self {
        Self {
            inner: Arc::new(Inner::new(client, cache_params)),
        }
    }

    pub async fn load_libraries(
        &self,
        hashes: &[TonHash],
    ) -> Result<Vec<ArcCell>, TonLibraryError> {
        let mut results = Vec::new();

        // If hashes exceed MAX_LIBS_REQUESTED, split them into chunks
        for chunk in hashes.chunks(Self::MAX_LIBS_REQUESTED) {
            let mut partial_result = self.inner.load_libraries_impl(chunk).await?;
            results.append(&mut partial_result);
        }
        Ok(results)
    }
}

impl Inner {
    fn new(client: &TonClient, cache_params: Option<LibraryCacheParams>) -> Self {
        let cache_params = cache_params.unwrap_or_default();

        let cache = Cache::builder()
            .max_capacity(cache_params.capacity)
            .time_to_live(cache_params.time_to_live)
            .build();

        Self {
            client: client.clone(),
            cache,
        }
    }

    async fn get_libs(
        &self,
        lib_ids: Vec<TonHash>,
    ) -> Result<HashMap<TonHash, ArcCell>, TonLibraryError> {
        let mut result_libs = HashMap::new();

        let cached_libs_future = lib_ids.into_iter().map(|key| async move {
            let maybe_cached = self.cache.get(&key).await;
            (key, maybe_cached)
        });
        let maybe_cached_libs = join_all(cached_libs_future).await;

        let mut hashes_to_load = vec![];
        for (hash, value) in maybe_cached_libs {
            match value {
                Some(lib) => {
                    result_libs.insert(hash, lib);
                }
                None => {
                    hashes_to_load.push(hash);
                }
            }
        }

        let blockchain_libs = self.load_libraries_impl(hashes_to_load.as_slice()).await?;
        result_libs.extend(blockchain_libs.iter().map(|l| (l.cell_hash(), l.clone())));

        self.insert_to_lib_cache(blockchain_libs).await?;
        Ok(result_libs)
    }

    async fn insert_to_lib_cache(&self, libs: Vec<ArcCell>) -> Result<(), TonLibraryError> {
        let mut cache_insert_futures = Vec::with_capacity(libs.len());
        for lib in libs {
            cache_insert_futures.push(self.cache.insert(lib.cell_hash(), lib.clone()));
        }
        join_all(cache_insert_futures).await;
        Ok(())
    }

    async fn load_libraries_impl(
        &self,
        hashes: &[TonHash],
    ) -> Result<Vec<ArcCell>, TonLibraryError> {
        let library_list: Vec<_> = hashes.iter().map(TonLibraryId::from).collect();
        let library_result = self
            .client
            .smc_get_libraries(&library_list)
            .await
            .map_err(|e| TonLibraryError::TonClientError(e.to_string()))?;

        let libraries: Vec<ArcCell> = library_result
            .result
            .into_iter()
            .map(|lib| BagOfCells::parse(&lib.data)?.single_root())
            .collect::<Result<_, _>>()?;

        Ok(libraries)
    }
}

use std::collections::HashMap;
use std::sync::Mutex;
use anyhow::Result;
use bytes::Bytes;
use libipld::Cid;
use log::trace;
use reqwest;
use once_cell::sync::Lazy;
use wnfs::common::{BlockStore, CODEC_DAG_CBOR};
use crate::{blockstore::FFIStore, private_forest::FFIFriendlyBlockStore};
use sha2::{Sha256, Digest};
use tokio::time::Duration;
// Global memory store
static MEMORY_STORE: Lazy<Mutex<HashMap<String, Vec<u8>>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

#[derive(Clone)]
pub struct WebBlockStore {
    pub gateway_url: String,
    pub codec: u64,
}

impl WebBlockStore {
    pub fn new(gateway_url: String, codec: u64) -> Self {
        Self {
            gateway_url,
            codec,
        }
    }

    fn cid_to_string(cid: &[u8]) -> String {
        Cid::try_from(cid).unwrap().to_string()
    }
}

#[async_trait::async_trait(?Send)]
impl<'a> FFIStore<'a> for WebBlockStore {
    fn get_block(&self, cid: Vec<u8>) -> Result<Vec<u8>> {
        // Use tokio::task::block_in_place to properly handle blocking operations in async context
        tokio::task::block_in_place(|| {
            let cid_string = Self::cid_to_string(&cid);
            
            if let Some(data) = MEMORY_STORE.lock().unwrap().get(&cid_string) {
                trace!("Retrieved from memory store: {}", cid_string);
                return Ok(data.clone());
            }

            let url = format!("{}/{}?raw", self.gateway_url, cid_string);
            trace!("Fetching from remote: {}", url);

            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?;
            
            let response = client
                .get(&url)
                .header("Accept", "*/*")
                .header("Content-Type", "application/octet-stream")
                .send()?
                .bytes()?;

            let data = response.to_vec();
            trace!("Result of get: {:?}", data);
            MEMORY_STORE.lock().unwrap().insert(cid_string, data.clone());
            Ok(data)
        })
    }

    fn put_block(&self, cid: Vec<u8>, bytes: Vec<u8>) -> Result<()> {
        let cid_string = Self::cid_to_string(&cid);
        MEMORY_STORE.lock().unwrap().insert(cid_string, bytes);
        Ok(())
    }
}


#[cfg(test)]
mod tests {

    use crate::private_forest::PrivateDirectoryHelper;
    use env_logger::{Builder, Env};

    use super::*;

    use once_cell::sync::Lazy;

    static INIT_LOGGER: Lazy<()> = Lazy::new(|| {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
            .is_test(true)
            .try_init()
            .ok();
    });

    fn setup() {
        Lazy::force(&INIT_LOGGER);
    }

    fn clear_memory_store() {
        MEMORY_STORE.lock().unwrap().clear();
    }

    const TEST_WNFS_KEY: &'static str = "253,78,31,107,225,226,191,37,170,183,150,195,158,20,19,61,113,210,91,33,107,114,123,83,39,213,125,249,10,28,254,218,113,89,93,240,221,54,221,217,134,126,143,122,131,4,215,228,120,80,219,105,171,10,63,167,39,216,151,74,134,43,1,235";
    const TEST_CID: &str = "bafyr4iantpew6r3hd6rsu5l5ioffbrryovkwlry7niwlqybph4qo75um5m";

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_with_wnfs_key() {
        setup();
        trace!("test_load_with_wnfs_key started");
        let store = WebBlockStore::new(
            "https://ipfs.cloud.fx.land/gateway".to_string(),
            CODEC_DAG_CBOR
        );
        let mut blockstore = FFIFriendlyBlockStore::new(Box::new(store));
        // Hash the 64-byte key to get 32-byte key
        let mut hasher = Sha256::new();
        let wnfs_key_b = TEST_WNFS_KEY.as_bytes();
        hasher.update(&wnfs_key_b);
        let hash32 = hasher.finalize();
        let wnfs_key = hash32.as_slice();
        trace!("wnfs key is: {:?}", wnfs_key);
        let cid = Cid::try_from(TEST_CID).unwrap();
        let result = PrivateDirectoryHelper::load_with_wnfs_key(
            &mut blockstore,
            cid,
            wnfs_key.to_vec()
        ).await;

        assert!(result.is_ok(), "Failed to load with WNFS key: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_load_private_forest() {
        trace!("test_load_private_forest started");
        let store = WebBlockStore::new(
            "https://ipfs.cloud.fx.land/gateway".to_string(),
            CODEC_DAG_CBOR
        );
        let blockstore = FFIFriendlyBlockStore::new(Box::new(store));

        let cid = Cid::try_from(TEST_CID).unwrap();
        trace!("loading {:?}", cid);
        let result = PrivateDirectoryHelper::load_private_forest(blockstore, cid).await;

        assert!(result.is_ok(), "Failed to load private forest: {:?}", result.err());
        
        // Verify the loaded forest
        if let Ok(forest) = result {
            // Add specific forest verification here
            trace!("forest ok");
        }
    }
}

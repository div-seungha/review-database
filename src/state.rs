use super::{event, IndexedMap, IndexedMultimap, IndexedSet, Map};
use anyhow::Result;
use std::path::Path;

// Key-value map names in `Database`.
pub(super) const ACCESS_TOKENS: &str = "access_tokens";
pub(super) const ACCOUNTS: &str = "accounts";
pub(super) const ACCOUNT_POLICY: &str = "account policy";
pub(super) const ALLOW_NETWORKS: &str = "allow networks";
pub(super) const BLOCK_NETWORKS: &str = "block networks";
pub(super) const CUSTOMERS: &str = "customers";
pub(super) const DATA_SOURCES: &str = "data sources";
pub(super) const FILTERS: &str = "filters";
pub(super) const MODEL_INDICATORS: &str = "model indicators";
const META: &str = "meta";
pub(super) const NETWORKS: &str = "networks";
pub(super) const NODES: &str = "nodes";
pub(super) const SAMPLING_POLICY: &str = "sampling policy";
pub(super) const TEMPLATES: &str = "templates";
pub(super) const TIDB: &str = "TI database";
pub(super) const TOR_EXIT_NODES: &str = "Tor exit nodes";
pub(super) const TRAFFIC_FILTER_RULES: &str = "traffic filter rules";
pub(super) const TRIAGE_POLICY: &str = "triage policy";
pub(super) const TRIAGE_RESPONSE: &str = "triage response";
pub(super) const TRUSTED_DNS_SERVERS: &str = "trusted DNS servers";

const MAP_NAMES: [&str; 20] = [
    ACCESS_TOKENS,
    ACCOUNTS,
    ACCOUNT_POLICY,
    ALLOW_NETWORKS,
    BLOCK_NETWORKS,
    CUSTOMERS,
    DATA_SOURCES,
    FILTERS,
    MODEL_INDICATORS,
    META,
    NETWORKS,
    NODES,
    SAMPLING_POLICY,
    TEMPLATES,
    TIDB,
    TOR_EXIT_NODES,
    TRAFFIC_FILTER_RULES,
    TRIAGE_POLICY,
    TRIAGE_RESPONSE,
    TRUSTED_DNS_SERVERS,
];

// Keys for the meta map.
pub(super) const EVENT_TAGS: &[u8] = b"event tags";
pub(super) const NETWORK_TAGS: &[u8] = b"network tags";
pub(super) const WORKFLOW_TAGS: &[u8] = b"workflow tags";

#[allow(clippy::module_name_repetitions)]
pub struct StateDb {
    inner: rocksdb::OptimisticTransactionDB,
}

impl StateDb {
    pub fn open(path: &Path) -> Result<Self, anyhow::Error> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let db = rocksdb::OptimisticTransactionDB::open_cf(&opts, path, MAP_NAMES)?;
        Ok(Self { inner: db })
    }

    pub fn events(&self) -> event::EventDb {
        event::EventDb::new(&self.inner)
    }

    pub fn map(&self, name: &str) -> Option<Map> {
        Map::new(&self.inner, name).ok()
    }

    pub fn indexed_map(&self, name: &str) -> Option<IndexedMap> {
        IndexedMap::new(&self.inner, name).ok()
    }

    pub fn indexed_multimap(&self, name: &str) -> Option<IndexedMultimap> {
        IndexedMultimap::new(&self.inner, name).ok()
    }

    pub fn indexed_set(&self, name: &'static [u8]) -> Option<IndexedSet> {
        IndexedSet::new(&self.inner, META, name).ok()
    }

    pub fn create_new_backup_flush(
        &self,
        engine: &Path,
        flush: bool,
        num_of_backups_to_keep: u32,
    ) -> Result<()> {
        let opts = rocksdb::backup::BackupEngineOptions::new(engine)?;
        let db_env = rocksdb::Env::new()?;
        let mut engine = rocksdb::backup::BackupEngine::open(&opts, &db_env)?;
        engine.create_new_backup_flush(&self.inner, flush)?;
        Ok(engine.purge_old_backups(num_of_backups_to_keep as usize)?)
    }

    pub fn restore_from_latest_backup(&self, location: &Path) -> Result<()> {
        use rocksdb::backup::{BackupEngine, BackupEngineOptions, RestoreOptions};
        let opts = BackupEngineOptions::new(location)?;
        let db_env = rocksdb::Env::new()?;
        let mut engine = BackupEngine::open(&opts, &db_env)?;
        let opts = RestoreOptions::default();
        Ok(engine.restore_from_latest_backup(self.inner.path(), location, &opts)?)
    }
}

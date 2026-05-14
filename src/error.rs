use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid shard counts: data={data}, parity={parity}")]
    InvalidShardCounts { data: usize, parity: usize },

    #[error("wrong number of shards: expected {expected}, got {actual}")]
    WrongShardCount { expected: usize, actual: usize },

    #[error("shards have mismatched sizes")]
    ShardSizeMismatch,

    #[error("empty shard")]
    EmptyShard,

    #[error("not enough shards present: have {present}, need {needed}")]
    TooFewShards { present: usize, needed: usize },

    /// Should never happen with a well-formed encoding matrix.
    #[error("submatrix is singular; this is a bug, please report it")]
    SingularMatrix,
}

pub type Result<T> = core::result::Result<T, Error>;

//! Data engineering pipeline — ingestion, validation, transformation, versioning.

pub mod lineage;
pub mod schema;
pub mod source;
pub mod storage;
pub mod transform;
pub mod validate;

pub use lineage::DataLineage;
pub use schema::{ColumnType, SchemaDefinition};
pub use source::{
    ApiSource, CsvSource, DataBatch, DataSource, DataSourceInfo, DataSourceType, HuggingFaceSource,
    JsonSource, JsonlSource, ParquetSource, SqliteSource,
};
pub use storage::{DatasetEntry, DatasetRegistry};
pub use transform::TransformPipeline;
pub use validate::DataQualityReport;

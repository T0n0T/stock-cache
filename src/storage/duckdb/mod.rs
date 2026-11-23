//! DuckDB persistence facade.
//!
//! Schema creation, source/calculation writing, and checkpoint reads live in
//! separate modules so application workflows depend on a stable storage API.

mod calculation_state;
mod market_data;
mod quack;
mod schema;
mod writer;

pub use calculation_state::load_indicator_checkpoints;
pub use market_data::table_count;
pub use quack::QuackServer;
pub use schema::{
    checkpoint, create_schema, table_columns, table_keys, with_connection,
    INDUSTRY_MONEYFLOW_TABLE, INDUSTRY_MONEYFLOW_VERSION,
};
pub use writer::write_jsonl;

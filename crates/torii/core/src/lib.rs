use serde::Deserialize;
use sqlx::FromRow;

use crate::types::SQLFieldElement;

pub mod engine;
pub mod processors;
pub mod simple_broker;
pub mod sql;
pub mod types;

#[allow(dead_code)]
#[derive(FromRow, Deserialize)]
pub struct World {
    #[sqlx(try_from = "String")]
    world_address: SQLFieldElement,
    #[sqlx(try_from = "String")]
    world_class_hash: SQLFieldElement,
    #[sqlx(try_from = "String")]
    executor_address: SQLFieldElement,
    #[sqlx(try_from = "String")]
    executor_class_hash: SQLFieldElement,
}

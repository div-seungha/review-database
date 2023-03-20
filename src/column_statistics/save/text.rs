use crate::{backends::Transaction, Type};
use std::convert::TryFrom;
use structured::{ColumnStatistics, Element};
use tracing::error;

pub(super) async fn insert_text<'a>(
    transaction: &Transaction<'a>,
    description_id: i32,
    column_stats: &ColumnStatistics,
    mode: &str,
) {
    if let Err(e) = transaction
        .insert_into(
            "description_text",
            &[("description_id", Type::INT4), ("mode", Type::TEXT)],
            &[&description_id, &mode],
        )
        .await
    {
        error!("Failed to insert description_text: {:#}", e);
    }

    for e in column_stats.n_largest_count.top_n() {
        let value = if let Element::Text(text) = &e.value {
            Some(text.clone())
        } else {
            None
        };
        let count = i64::try_from(e.count).expect("Must be less than i64::MAX");

        if let Err(e) = transaction
            .insert_into(
                "top_n_text",
                &[
                    ("description_id", Type::INT4),
                    ("value", Type::TEXT),
                    ("count", Type::INT8),
                ],
                &[&description_id, &value, &count],
            )
            .await
        {
            error!("Failed to insert top_n_binary: {:#}", e);
        }
    }
}

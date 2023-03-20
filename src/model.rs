use super::{tokio_postgres::types::ToSql, Database, Error, OrderDirection, Type};
use num_traits::ToPrimitive;
use serde::Deserialize;

const DEFAULT_MAX_EVENT_ID_NUM_I32: i32 = 25;
const DEFAULT_MAX_EVENT_ID_NUM_U32: u32 = 25;
const LIMIT_MAX_EVENT_ID_NUM: i32 = 100;

#[derive(Deserialize)]
pub struct Model {
    pub id: i32,
    pub name: String,
    pub data_source_id: i32,
}

impl Database {
    const CSV_COLUMN_TYPES: &[&'static str] = &[
        "binary", "datetime", "enum", "float", "int", "ipaddr", "text",
    ];

    fn type_tables(prefix: &str) -> Vec<String> {
        Self::CSV_COLUMN_TYPES
            .iter()
            .map(|t| format!("{prefix}_{t}"))
            .collect()
    }

    pub async fn add_model(
        &self,
        name: &str,
        kind: &str,
        serialized_classifier: &[u8],
        max_event_id_num: u32,
        data_source_id: i32,
    ) -> Result<i32, Error> {
        let max_event_id_num = i32::min(
            max_event_id_num
                .to_i32()
                .unwrap_or(DEFAULT_MAX_EVENT_ID_NUM_I32),
            LIMIT_MAX_EVENT_ID_NUM,
        );

        let conn = self.pool.get().await?;
        let n = conn
            .insert_into(
                "model",
                &[
                    ("name", Type::TEXT),
                    ("kind", Type::TEXT),
                    ("classifier", Type::BYTEA),
                    ("max_event_id_num", Type::INT4),
                    ("data_source_id", Type::INT4),
                ],
                &[
                    &name,
                    &kind,
                    &serialized_classifier,
                    &max_event_id_num,
                    &data_source_id,
                ],
            )
            .await
            .map_err(|_| Error::InvalidInput(format!("model \"{name}\" already exists")))?;
        if n == 0 {
            Err(Error::InvalidInput("failed to insert model".into()))
        } else {
            Ok(n)
        }
    }

    pub async fn delete_model(&self, name: &str) -> Result<(), Error> {
        let conn = self.pool.get().await?;
        let query_result = conn
            .select_one_opt_from::<i32>("model", &["id"], &[("name", Type::TEXT)], &[&name])
            .await?;
        let Some(id) = query_result else {
            return Err(Error::InvalidInput(format!("The model {name} not found")));
        };
        conn.delete_from("model", &[("id", Type::INT4)], &[&id])
            .await?;
        conn.delete_from("csv_column_list", &[("model_id", Type::INT4)], &[&id])
            .await?;
        conn.delete_from("csv_column_extra", &[("model_id", Type::INT4)], &[&id])
            .await?;

        self.delete_csv_entries_under_model_name(name).await?;

        self.delete_stats(id).await?;

        Ok(())
    }

    async fn delete_csv_entries_under_model_name(&self, model_name: &str) -> Result<(), Error> {
        let conn = self.pool.get().await?;

        let tables_and_key: &[(&str, &str)] =
            &[("csv_indicator", "name"), ("csv_whitelist", "name")];

        for (t, k) in tables_and_key {
            conn.delete_from(t, &[(k, Type::TEXT)], &[&model_name])
                .await?;
        }

        Ok(())
    }

    async fn delete_stats(&self, id: i32) -> Result<(), Error> {
        let conn = self.pool.get().await?;

        conn.delete_from("outlier", &[("model_id", Type::INT4)], &[&id])
            .await?;

        let cluster_ids: Vec<i32> = conn
            .select_in(
                "cluster",
                &["id"],
                &[("model_id", Type::INT4)],
                &[],
                &[],
                &[&id],
            )
            .await?;
        if cluster_ids.is_empty() {
            return Ok(());
        }

        conn.delete_from("cluster", &[("model_id", Type::INT4)], &[&id])
            .await?;

        conn.delete_in(
            "time_series",
            &[],
            &[("cluster_id", Type::INT4_ARRAY)],
            &[&cluster_ids],
        )
        .await?;

        let event_range_ids: Vec<i32> = conn
            .select_in(
                "event_range",
                &["id"],
                &[],
                &[("cluster_id", Type::INT4_ARRAY)],
                &[],
                &[&cluster_ids],
            )
            .await?;
        conn.delete_in(
            "event_range",
            &[],
            &[("id", Type::INT4_ARRAY)],
            &[&event_range_ids],
        )
        .await?;

        let column_description_ids: Vec<i32> = conn
            .select_in(
                "column_description",
                &["id"],
                &[],
                &[("event_range_id", Type::INT4_ARRAY)],
                &[],
                &[&event_range_ids],
            )
            .await?;
        conn.delete_in(
            "column_description",
            &[],
            &[("id", Type::INT4_ARRAY)],
            &[&event_range_ids],
        )
        .await?;

        let prefixes = &["top_n", "description"];
        for p in prefixes {
            let tables = Self::type_tables(p);
            for t in tables {
                conn.delete_in(
                    &t,
                    &[],
                    &[("description_id", Type::INT4_ARRAY)],
                    &[&column_description_ids],
                )
                .await?;
            }
        }

        Ok(())
    }

    pub async fn count_models(&self) -> Result<i64, Error> {
        let conn = self.pool.get().await?;
        conn.count("model", &[], &[], &[]).await
    }

    pub async fn get_max_event_id_num(&self, model_name: &str) -> Result<i32, Error> {
        let conn = self.pool.get().await?;
        conn.select_one_from(
            "model",
            &["max_event_id_num"],
            &[("name", Type::TEXT)],
            &[&model_name],
        )
        .await
    }

    pub async fn load_model(&self, id: i32) -> Result<Model, Error> {
        let conn = self.pool.get().await?;
        conn.select_one_from::<Model>(
            "model",
            &["id", "name", "data_source_id"],
            &[("id", super::Type::INT4)],
            &[&id],
        )
        .await
    }

    #[allow(clippy::type_complexity)]
    pub async fn load_model_by_name(
        &self,
        name: &str,
    ) -> Result<(i32, String, Vec<u8>, u32), Error> {
        #[derive(Deserialize)]
        struct Model {
            id: i32,
            kind: String,
            max_event_id_num: i32,
            #[serde(with = "serde_bytes")]
            classifier: Vec<u8>,
        }

        let conn = self.pool.get().await?;
        let model = conn
            .select_one_from::<Model>(
                "model",
                &["id", "kind", "max_event_id_num", "classifier"],
                &[("name", super::Type::TEXT)],
                &[&name],
            )
            .await?;
        Ok((
            model.id,
            model.kind,
            model.classifier,
            model
                .max_event_id_num
                .to_u32()
                .unwrap_or(DEFAULT_MAX_EVENT_ID_NUM_U32),
        ))
    }

    pub async fn load_models(
        &self,
        after: &Option<(i32, String)>,
        before: &Option<(i32, String)>,
        is_first: bool,
        limit: usize,
    ) -> Result<Vec<Model>, Error> {
        let conn = self.pool.get().await?;
        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        if let Some(cursor) = after {
            params.push(&cursor.1);
            params.push(&cursor.0);
        }
        if let Some(cursor) = before {
            params.push(&cursor.1);
            params.push(&cursor.0);
        }
        conn.select_slice(
            "model",
            &["id", "name", "data_source_id"],
            &[],
            &[],
            &params,
            &("name", Type::TEXT),
            OrderDirection::Asc,
            (after.is_some(), before.is_some()),
            is_first,
            limit,
        )
        .await
    }

    pub async fn update_model(
        &self,
        name: &str,
        kind: &str,
        serialized_classifier: Vec<u8>,
        max_event_id_num: u32,
        data_source_id: i32,
    ) -> Result<i32, Error> {
        let max_event_id_num = i32::min(
            max_event_id_num
                .to_i32()
                .unwrap_or(DEFAULT_MAX_EVENT_ID_NUM_I32),
            LIMIT_MAX_EVENT_ID_NUM,
        );

        let conn = self.pool.get().await?;
        let id = conn
            .select_one_from::<i32>("model", &["id"], &[("name", Type::TEXT)], &[&name])
            .await
            .map_err(|e| Error::InvalidInput(format!("cannot find model \"{name}\": {e}")))?;
        conn.update(
            "model",
            id,
            &[
                ("kind", super::Type::TEXT),
                ("classifier", Type::BYTEA),
                ("max_event_id_num", Type::INT4),
                ("data_source_id", Type::INT4),
            ],
            &[
                &kind,
                &serialized_classifier,
                &max_event_id_num,
                &data_source_id,
            ],
        )
        .await
        .map_err(|e| Error::InvalidInput(format!("failed to update model \"{name}\": {e}")))?;
        Ok(id)
    }
}

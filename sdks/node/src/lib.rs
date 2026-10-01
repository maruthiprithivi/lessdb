//! LessDB Node.js SDK — in-process analytical database.
//!
//! ```js
//! const { open } = require("@lessdb/node");
//! const db = open("mydb");
//! db.createTable("CREATE TABLE t (x Int64) ENGINE=Firefly ORDER BY (x)");
//! console.log(await db.queryJson("SELECT sum(x) FROM t"));
//! const ipc = await db.queryArrow("SELECT * FROM t"); // Arrow IPC buffer
//! ```

use std::sync::Arc;

use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

fn to_napi_err(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// An embedded LessDB connection.
#[napi]
pub struct Connection {
    session: Arc<less_query::LessSession>,
}

#[napi]
impl Connection {
    /// Open (creating if needed) a database directory.
    #[napi(factory)]
    pub fn open(path: Option<String>) -> napi::Result<Self> {
        let dir = path.unwrap_or_else(|| ".less".to_string());
        let engine = less_engine::LessEngine::open_local(dir).map_err(to_napi_err)?;
        let session = less_query::LessSession::new(engine).map_err(to_napi_err)?;
        Ok(Self {
            session: Arc::new(session),
        })
    }

    /// Run SQL, returning rows as a JSON array of objects.
    #[napi]
    pub async fn query_json(&self, sql: String) -> napi::Result<String> {
        let batches = self.session.sql_batches(&sql).await.map_err(to_napi_err)?;
        let mut buf = Vec::new();
        {
            let mut writer =
                arrow_json::writer::Writer::<_, arrow_json::writer::JsonArray>::new(&mut buf);
            for b in &batches {
                writer.write(b).map_err(to_napi_err)?;
            }
            writer.finish().map_err(to_napi_err)?;
        }
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    /// Run SQL, returning an Arrow IPC stream buffer (feed to
    /// `apache-arrow`'s `Table.from(...)`).
    #[napi]
    pub async fn query_arrow(&self, sql: String) -> napi::Result<Buffer> {
        let batches = self.session.sql_batches(&sql).await.map_err(to_napi_err)?;
        let schema = batches
            .first()
            .map(|b| b.schema())
            .unwrap_or_else(|| Arc::new(arrow::datatypes::Schema::empty()));
        let mut buf = Vec::new();
        {
            let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buf, &schema)
                .map_err(to_napi_err)?;
            for b in &batches {
                writer.write(b).map_err(to_napi_err)?;
            }
            writer.finish().map_err(to_napi_err)?;
        }
        Ok(buf.into())
    }

    /// Create a table from LessDB DDL.
    #[napi]
    pub fn create_table(&self, ddl: String) -> napi::Result<String> {
        let parsed = less_catalog::ddl::parse_create(&ddl).map_err(to_napi_err)?;
        let def = parsed.to_def();
        self.session
            .engine()
            .create_table(def.clone())
            .map_err(to_napi_err)?;
        self.session.refresh().map_err(to_napi_err)?;
        Ok(def.name)
    }

    /// List table names.
    #[napi]
    pub fn tables(&self) -> napi::Result<Vec<String>> {
        self.session.engine().tables().map_err(to_napi_err)
    }

    /// Describe a table (JSON manifest).
    #[napi]
    pub fn describe(&self, table: String) -> napi::Result<String> {
        let def = self.session.engine().table(&table).map_err(to_napi_err)?;
        serde_json::to_string_pretty(&def).map_err(to_napi_err)
    }

    /// Merge parts / enforce UNIQUE constraints.
    #[napi]
    pub async fn optimize(&self, table: String) -> napi::Result<Option<String>> {
        self.session
            .engine()
            .optimize(&table)
            .map_err(to_napi_err)
            .map(|meta| meta.map(|m| m.name))
    }
}

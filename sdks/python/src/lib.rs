//! LessDB Python SDK — in-process analytical database, DuckDB style.
//!
//! ```python
//! import lessdb
//! db = lessdb.open("mydb")                     # or open() -> .less
//! db.create_table("CREATE TABLE t (x Int64) ENGINE = Firefly ORDER BY (x)")
//! db.insert_json("t", [{"x": 1}, {"x": 2}])
//! print(db.sql("SELECT sum(x) FROM t"))        # JSON array
//! table = db.query_arrow("SELECT * FROM t")    # pyarrow.Table (needs pyarrow)
//! ```

use std::sync::Arc;
use std::sync::OnceLock;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes};

use less_common::LessError;

fn to_pyerr(e: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
}

/// Shared tokio runtime for async query execution.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("lessdb-py")
            .enable_all()
            .build()
            .expect("failed to build lessdb tokio runtime")
    })
}

/// An embedded LessDB connection.
#[pyclass(name = "Connection")]
struct Connection {
    session: Arc<less_query::LessSession>,
}

#[pymethods]
impl Connection {
    /// Open (creating if needed) a database directory.
    #[new]
    #[pyo3(signature = (path=None))]
    fn new(path: Option<String>) -> PyResult<Self> {
        let dir = path.unwrap_or_else(|| ".less".to_string());
        let engine = less_engine::LessEngine::open_local(dir).map_err(to_pyerr)?;
        let session = less_query::LessSession::new(engine).map_err(to_pyerr)?;
        Ok(Self {
            session: Arc::new(session),
        })
    }

    /// Run SQL, returning rows as a JSON array of objects.
    fn sql(&self, sql: &str) -> PyResult<String> {
        self.query(sql)
    }

    /// Run SQL, returning rows as a JSON array of objects.
    fn query(&self, sql: &str) -> PyResult<String> {
        let batches = runtime()
            .block_on(self.session.sql_batches(sql))
            .map_err(to_pyerr)?;
        let mut buf = Vec::new();
        {
            let mut writer =
                arrow_json::writer::Writer::<_, arrow_json::writer::JsonArray>::new(&mut buf);
            for b in &batches {
                writer.write(b).map_err(to_pyerr)?;
            }
            writer.finish().map_err(to_pyerr)?;
        }
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    /// Run SQL and return a `pyarrow.Table` (requires pyarrow installed).
    fn query_arrow(&self, py: Python<'_>, sql: &str) -> PyResult<Py<PyAny>> {
        let batches = runtime()
            .block_on(self.session.sql_batches(sql))
            .map_err(to_pyerr)?;
        let schema = batches
            .first()
            .map(|b| b.schema())
            .unwrap_or_else(|| Arc::new(arrow::datatypes::Schema::empty()));
        let mut buf = Vec::new();
        {
            let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buf, &schema)
                .map_err(to_pyerr)?;
            for b in &batches {
                writer.write(b).map_err(to_pyerr)?;
            }
            writer.finish().map_err(to_pyerr)?;
        }
        let pa = py.import("pyarrow").map_err(|e| {
            pyo3::exceptions::PyImportError::new_err(format!(
                "pyarrow is required for query_arrow: {e}"
            ))
        })?;
        let ipc = pa.getattr("ipc")?;
        let bytes = PyBytes::new(py, &buf);
        let stream = ipc.call_method1("open_stream", (bytes,))?;
        stream.call_method0("read_all").map(|t| t.unbind())
    }

    /// Create a table from LessDB DDL
    /// (e.g. `CREATE TABLE t (x Int64) ENGINE=Firefly ORDER BY (x)`).
    fn create_table(&self, ddl: &str) -> PyResult<String> {
        let parsed = less_catalog::ddl::parse_create(ddl).map_err(to_pyerr)?;
        let def = parsed.to_def();
        self.session
            .engine()
            .create_table(def.clone())
            .map_err(to_pyerr)?;
        self.session.refresh().map_err(to_pyerr)?;
        Ok(def.name)
    }

    /// Insert rows from a JSON array of objects (or NDJSON string).
    /// Accepts a JSON string, a list of dicts, or a list of tuples.
    /// Returns the number of rows inserted.
    fn insert_json(&self, py: Python<'_>, table: &str, json: &Bound<'_, PyAny>) -> PyResult<u64> {
        let json_str: String = if let Ok(s) = json.downcast::<pyo3::types::PyString>() {
            s.to_string_lossy().into_owned()
        } else {
            py.import("json")?
                .call_method1("dumps", (json,))?
                .extract::<String>()?
        };
        let def = self.session.engine().table(table).map_err(to_pyerr)?;
        let schema = def.arrow_schema();
        // arrow-json 59 reads NDJSON; convert JSON arrays first.
        let ndjson: String = if json_str.trim_start().starts_with('[') {
            let values: Vec<serde_json::Value> =
                serde_json::from_str(&json_str).map_err(to_pyerr)?;
            let mut out = String::with_capacity(json_str.len());
            for v in values {
                out.push_str(&v.to_string());
                out.push('\n');
            }
            out
        } else {
            json_str.clone()
        };
        let reader = arrow_json::reader::ReaderBuilder::new(schema)
            .with_batch_size(262_144)
            .build(std::io::Cursor::new(ndjson.into_bytes()))
            .map_err(|e| to_pyerr(LessError::Arrow(e)))?;
        let mut rows = 0u64;
        for batch in reader {
            let batch = batch.map_err(|e| to_pyerr(LessError::Arrow(e)))?;
            rows += self
                .session
                .engine()
                .insert(table, batch)
                .map_err(to_pyerr)? as u64;
        }
        self.session.engine().flush(table).map_err(to_pyerr)?;
        Ok(rows)
    }

    /// List table names.
    fn tables(&self) -> PyResult<Vec<String>> {
        self.session.engine().tables().map_err(to_pyerr)
    }

    /// Describe a table (JSON manifest).
    fn describe(&self, table: &str) -> PyResult<String> {
        let def = self.session.engine().table(table).map_err(to_pyerr)?;
        serde_json::to_string_pretty(&def).map_err(to_pyerr)
    }

    /// Merge parts / enforce UNIQUE constraints.
    fn optimize(&self, table: &str) -> PyResult<Option<String>> {
        self.session
            .engine()
            .optimize(table)
            .map_err(to_pyerr)
            .map(|meta| meta.map(|m| m.name))
    }

    fn __repr__(&self) -> String {
        format!(
            "lessdb.Connection({} tables)",
            self.session.engine().tables().map(|t| t.len()).unwrap_or(0)
        )
    }
}

/// Open a LessDB database directory (DuckDB-style entry point).
#[pyfunction]
#[pyo3(signature = (path=None))]
fn open(path: Option<String>) -> PyResult<Connection> {
    Connection::new(path)
}

/// Open a LessDB database directory (alias for `open`).
#[pyfunction]
#[pyo3(signature = (path=None))]
fn connect(path: Option<String>) -> PyResult<Connection> {
    Connection::new(path)
}

/// LessDB Python module.
#[pymodule]
fn lessdb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Connection>()?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(connect, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

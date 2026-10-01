# Embedded analytics (in-process database)

**What**: Ship an analytical database *inside* your product — Python or
Node SDK, zero servers, Arrow-native — then grow the same data into a
shared, served deployment without changing file formats.

**Why LessDB**:

- One call opens an engine: `lessdb.open("analytics.less")`. No ports,
  no daemons, no infra tickets.
- The SDK speaks Arrow, so results flow straight into pandas/Polars/
  PyArrow with no serialization tax.
- Same tables, same format from laptop to server: start embedded, move
  to `lessdb server` or FireflyCloud object storage when you grow.

**How, step by step**:

```python
# 1. open in-process
import lessdb
db = lessdb.open("analytics.less")

# 2. schema + data (Arrow batches stream straight in)
db.create_table("CREATE TABLE events (ts Timestamp, user_id Int64, event String) \
                 ENGINE = Firefly ORDER BY (event, ts)")
db.insert("events", arrow_batch)

# 3. query from application code — Arrow out
table = db.sql("SELECT event, count(*) FROM events \
                WHERE ts > now() - INTERVAL '1 day' GROUP BY event")
df = table.to_pandas()          # or .to_polars()
```

```js
// Node: same shape, same engine
const { LessDB } = require("lessdb");
const db = await LessDB.open("analytics.less");
const rows = await db.sql("SELECT event, count(*) FROM events GROUP BY event");
```

```sh
# 4. grow without changing anything: the same directory, now served
lessdb server --dir analytics.less --addr 0.0.0.0:7080
```

**Value**: you stop saying "we'll add analytics later" — the database
ships with the product, and the day you need shared/team access, the
data doesn't move an inch.

# Multi-node FireflyCloud (zero-replication horizontal scale)

LessDB scales horizontally without replicas or a quorum protocol:
durable state lives in shared object storage (S3, GCS, Azure, or any
S3-compatible store); compute nodes are stateless and interchangeable.

## The model

```
   compute A        compute B        compute C   …
   (stateless)      (stateless)      (stateless)
   local disk:      local disk:      local disk:
   buffers, caches, buffers, caches, buffers, caches
        └───────────────┬───────────────┘
                shared object storage
        catalog/<table>.json        (table manifests)
        tables/<t>/parts/<part>/*   (immutable data parts)
```

## Try it locally with file:// storage

```sh
lessdb init --dir /tmp/node-a --shared s3://lessdb/test
lessdb init --dir /tmp/node-b --shared s3://lessdb/test

lessdb create --dir /tmp/node-a \
  "CREATE TABLE t (id Int64, v Float64) ENGINE=FireflyCloud ORDER BY id"

# node B discovers the table purely by listing the shared store
lessdb tables --dir /tmp/node-b
lessdb insert --dir /tmp/node-a t --csv rows.csv
lessdb sql    --dir /tmp/node-b "SELECT count(*) FROM t"    # sees A's rows
```

`file://` storage is the development backend (under `<data_dir>/shared`);
point `--shared` at a real `s3://`/`gcs://`/`az://` URL for production.

## How concurrent writers stay safe

* **Part publication** is a conditional create of `meta.json` — readers
  only see parts whose metadata exists, and two writers publishing the
  same uuid-named part can't silently clobber each other.
* **Merges** first claim their inputs under
  `metastore/merge-claims/<table>/<part>` (10-minute TTL; expired claims
  are taken over where update-CAS exists). Two nodes can flush and
  OPTIMIZE concurrently — every row ends up exactly once.
* Proven by the `multi_writer_concurrent_flush_and_optimize` test in
  `less-engine` (two nodes, concurrent flushes + concurrent OPTIMIZE).

## Operational properties

* Kill any node: nothing durable is lost; a replacement with an empty
  local directory sees every table immediately.
* Reads on shared tables never touch local disk; the block cache (in-RAM
  LRU + optional disk tier) serves repeated scans.
* Current measured trade-off: with the default 256 MiB block cache,
  whole-part fetches make shared queries ~8x slower than local parts —
  lazy range reads through the object store are the active roadmap item
  (see benchmarks doc).

## Fan-out (sharded scans across nodes)

```sh
lessdb fanout --nodes http://a:7080,http://b:7080,http://c:7080 \
  "SELECT host, count(*), sum(cpu) FROM events GROUP BY host"
```

`lessdb_shard(col, i, n)` partitions parts by stable hash across nodes;
each node aggregates its shard and the coordinator merges the partials
(v1 subset: group columns + count/sum/min/max).

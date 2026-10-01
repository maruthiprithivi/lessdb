# Semantic search & RAG

**What**: Embeddings, nearest-neighbor search, and RAG retrieval — as
plain SQL, next to the very tables you'd join against. No separate
vector service, no serialization hop, no embedding silo.

**Why LessDB**:

- `vector_search('space', [...], k)` is a **SQL table function**: hits
  come back as rows you can filter, join, and aggregate like anything
  else.
- Exact flat search for correctness, IVF-PQ ANN for scale (0.87
  recall@10 at 8.7k QPS in benchmarks) — one API.
- Vectors live in the same database as your documents' metadata, so
  "semantic similarity + business filters" is one query, not two
  services and a merge.

**How, step by step**:

```sh
# 1. a vector space for document embeddings
lessdb vector create docs 768 --metric cosine

# 2. add embeddings with payloads (from your embedding model)
lessdb vector add docs \
  --vectors '[[0.1,0.2,...],[0.3,0.4,...]]' \
  --payloads '[{"title":"postmortem: failover"},{"title":"pricing FAQ"}]'
```

```sql
-- 3. retrieve: nearest neighbours, then filter/join like SQL
SELECT id, score, payload
FROM vector_search('docs', [0.11, 0.19, ...], 10)
WHERE payload LIKE '%postmortem%';

-- 4. RAG: hits joined against a real table for grounded answers
SELECT d.payload, inc.severity
FROM vector_search('docs', [0.11, 0.19, ...], 5) d
JOIN incidents inc ON inc.doc_id = d.id;
```

```sh
# 5. agents get the same search through MCP (vector_* tools) — audited
lessdb mcp --require-auth
```

**Value**: retrieval quality plus business filtering in one engine —
your RAG pipeline stops shipping vectors across three services and
starts being one auditable query.

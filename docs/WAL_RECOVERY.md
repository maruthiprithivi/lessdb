# Malformed WAL recovery

Local WAL-enabled startup rejects incomplete headers, impossible payload lengths,
invalid Arrow IPC, missing end markers, and trailing bytes swallowed by an expanded
record length. Validation includes records already covered by a part cutoff. The
malformed log remains intact for audited operator recovery; no partial prefix is
returned for that log and no new insert can be acknowledged by a failed open.

This intentionally trades automatic startup availability after a torn append for
preservation of acknowledged data and diagnostic evidence. There is no automatic
tail repair or new checksum format. An invalid-looking tail cannot safely be
assumed to be unacknowledged. Operators must preserve the original database/WAL
and diagnose the cause before any recovery edit. This patch does not grant
permission to truncate or delete data.

The unchanged baseline still has independent parent-directory/part publication
barrier, multipart atomicity, transaction/isolation and multi-process concurrency
limitations. A failed partial append/fsync does not poison an already running
engine in this port; preventing subsequent acknowledgements behind that failure
requires a separate runtime write gate. Outer payload length is checked against
remaining file bytes, but there is no nested IPC decoder allocation/memory cap.
The fix does not certify ACID, physical power-loss durability, arbitrary
bit-corruption detection, or filesystem-independent guarantees. Existing WAL file
fsync configuration is unchanged. No new dependency or background service is added.

## Regression gates (isolated Optimus checkout only)

```sh
cargo fmt --all --check
cargo test --locked -p less-engine --lib wal::tests -j1
cargo test --locked -p less-engine --test wal_recovery -j1
cargo test --locked -p less-engine --test corruption -j1
cargo clippy --locked --workspace --all-targets -j1 -- -D warnings
cargo test --locked --workspace -j1
cargo run --release --locked -p less-sqllogictest -j1 -- tests/sqllogictest
```

Malformed-header/payload/EOS/swallowed-record tests preserve WAL bytes. The writable
reopen regression starts from a flushed part, adds an incomplete tail (with and
without covered records), and requires startup rejection. Its alternative success
branch exposes the former lost-acknowledgement sequence. The OS remains alive;
this is process/reopen and malformed-file evidence, not a hardware power-cut test.
The standalone public port requires its own Linux receipts; private stacked-branch
receipts do not prove this exact tree. No performance claim accompanies this fix.

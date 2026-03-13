# Fix: global_spots unknown table identifier

**Date:** 2026-03-13T15:15:29Z

## Problem

Queries against the `global_spots` table were failing with:

```
DB::Exception: Unknown table expression identifier 'global_spots'
```

The ClickHouse client was configured with `WSPR_CLICKHOUSE_DB` (default: `wsprrs`)
via `.with_database()`.  When ClickHouse resolves an unqualified table name it
looks only inside the active database, so `global_spots` — which lives in a
*different* database — could not be found.

## Fix

Added `WSPR_GLOBAL_DB` environment variable support to `Config`:

- New field `Config::global_db: Option<String>` populated from `WSPR_GLOBAL_DB`.
- New method `Config::global_table_qualified() -> String` returns `"db.table"` when
  `WSPR_GLOBAL_DB` is set, or just `"table"` otherwise (backward-compatible).
- All three call sites in `server_fns.rs` (`get_global_map_spots`,
  `get_global_spots`, `get_reporter_suggestions`) now pass
  `config.global_table_qualified()` instead of `&config.global_table`.

## Configuration

Set `WSPR_GLOBAL_DB` in `.env` to the ClickHouse database containing the global
spots table, e.g.:

```env
WSPR_GLOBAL_DB=rx888-clickhouse
```

If `WSPR_GLOBAL_DB` is unset the behaviour is unchanged.

# Feat: separate WSPR_GLOBAL_SPOT_LIMIT for global queries

**Date:** 2026-03-13T16:10:00Z

## Problem

Global queries shared `WSPR_SPOT_LIMIT` with local queries.  The global
`global_spots` table is orders of magnitude larger (92 M+ rows/month), so
users who tuned `WSPR_SPOT_LIMIT` conservatively for their local table ended
up with a very small cap on global results.

## Change

Added a new config field `Config::global_spot_limit` populated from the
`WSPR_GLOBAL_SPOT_LIMIT` environment variable (default: `10000`).

`get_global_map_spots` and `get_global_spots` now pass `global_spot_limit`
instead of `spot_limit` to their respective query functions.  Local queries
(`get_map_spots`, `get_spots`) are unchanged.

## Configuration

Add to `.env`:

```env
WSPR_GLOBAL_SPOT_LIMIT=50000
```

If unset, defaults to `10000`.  `WSPR_SPOT_LIMIT` continues to control the
local receiver table cap and is unaffected.

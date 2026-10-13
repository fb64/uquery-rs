---
sidebar_position: 3
title: Response Formats
description: Get uQuery results as JSON, JSON Lines, CSV or Apache Arrow IPC. The format is negotiated from the HTTP Accept header and streamed.
---

# Response Formats

µQuery negotiates the response format from the HTTP `Accept` header. Four formats are supported.

## JSON

**`Accept: application/json`** (default)

Returns a JSON array of objects, one per row. This is the default when `Accept` is `*/*` or omitted.

```bash
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: application/json" \
  -d "SELECT 1 AS id, 'hello' AS msg"
```

```json
[{"id":1,"msg":"hello"}]
```

## JSON Lines

**`Accept: application/jsonlines`** or **`Accept: application/jsonl`**

Returns one JSON object per line (newline-delimited). Useful for streaming pipelines and log ingestion tools.

```bash
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: application/jsonlines" \
  -d "SELECT unnest([1, 2, 3]) AS n"
```

```
{"n":1}
{"n":2}
{"n":3}
```

## CSV

**`Accept: text/csv`**

Returns comma-separated values with a header row.

```bash
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: text/csv" \
  -d "SELECT 1 AS id, 'hello' AS msg"
```

```
id,msg
1,hello
```

## Apache Arrow IPC

**`Accept: application/vnd.apache.arrow.stream`**

Returns an [Arrow IPC stream](https://arrow.apache.org/docs/format/IPC.html) — the most efficient format for large result sets. Preserves native column types and is directly consumable by Arrow-compatible tools (Python `pyarrow`, Polars, DuckDB, etc.).

```bash
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: application/vnd.apache.arrow.stream" \
  -d "SELECT * FROM 'https://example.com/data.parquet' LIMIT 1000" \
  --output result.arrow
```

Reading the result in Python:

```python
import pyarrow.ipc as ipc

with ipc.open_stream("result.arrow") as reader:
    table = reader.read_all()
    print(table)
```

Or with Polars:

```python
import polars as pl

df = pl.read_ipc_stream("result.arrow")
print(df)
```

### Compression

Arrow IPC responses can be compressed at the Arrow level with ZSTD or LZ4. To ask for it, add a `codecs` parameter to the Arrow media type, listing the codecs your client can decode in order of preference:

```bash
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H 'Accept: application/vnd.apache.arrow.stream; codecs="zstd, lz4"' \
  -d "SELECT * FROM 'https://example.com/data.parquet' LIMIT 1000" \
  --output result.arrow
```

µQuery uses the first codec in the list that it supports and names it in the response header, for example `Content-Type: application/vnd.apache.arrow.stream; codecs=zstd`. These responses skip HTTP gzip, since the data is already compressed.

| Codec | `codecs` value | Best for |
|---|---|---|
| ZSTD | `zstd` | Smallest responses |
| LZ4 | `lz4` | Fastest compression and decoding |

Without `codecs`, or when none of the listed codecs is supported, the response is uncompressed Arrow IPC, which every Arrow reader can decode. HTTP gzip still applies when the client sends `Accept-Encoding: gzip`.

pyarrow and Polars decode both codecs automatically, so the reading code above works unchanged. For example, with `requests`:

```python
import pyarrow as pa
import requests

response = requests.post(
    "http://localhost:8080",
    data="SELECT * FROM 'https://example.com/data.parquet'",
    headers={"Accept": 'application/vnd.apache.arrow.stream; codecs="zstd"'},
)
table = pa.ipc.open_stream(response.content).read_all()
```

:::warning
JavaScript clients such as Apache Arrow JS don't decode compressed IPC buffers out of the box. Leave out `codecs` and rely on HTTP gzip instead.
:::

## Streaming behaviour

All formats are streamed incrementally. µQuery begins writing the response as soon as the first result batch is available — there is no buffering of the full result set. This means:

- Large queries start returning data immediately
- Memory usage stays bounded on the server side
- Clients should handle streaming reads for very large results

## Summary

| Accept header | Format | Best for |
|---|---|---|
| `application/json` | JSON array | General use, small results |
| `application/jsonlines` | JSON Lines | Streaming pipelines, log tools |
| `text/csv` | CSV | Spreadsheet import, data tools |
| `application/vnd.apache.arrow.stream` | Arrow IPC | Large results, typed data, analytics |

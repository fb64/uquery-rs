---
sidebar_position: 3
title: Build a Web App
description: Build a small web app that charts public Parquet and CSV datasets, with a few lines of JavaScript in front of uQuery.
---

import WebAppDiagram from '@site/src/components/WebAppDiagram';

# Build a Web App

µQuery answers SQL over HTTP, so a web app only needs a few lines of server code to query data files. This tutorial runs the small app of the [`examples/web-app`](https://github.com/fb64/uquery-rs/tree/main/examples/web-app) folder, which charts two public datasets:

- NYC taxi trips, from a remote Parquet file
- CO₂ emissions by country, from a remote CSV file

## Prerequisites

- Git
- Podman with [podman-compose](https://github.com/containers/podman-compose), or Docker with Compose

## Start the app

```bash
git clone https://github.com/fb64/uquery-rs.git
cd uquery-rs/examples/web-app
podman compose up
```

With Docker, run `docker compose up` instead.

Open [http://localhost:8000](http://localhost:8000). The taxi chart shows the trips of each hour of the day, across about 3 million rows. Choosing another country in the list redraws the emissions chart.

## How it works

The browser never talks to µQuery. It calls the routes of a small server, and that server holds the SQL:

<WebAppDiagram />

Anyone can change the code of a web page, so SQL written in a page could be replaced with any other query. µQuery also [rejects requests sent by web browsers](../configuration.md#browser-clients), so that other websites can't query it through your visitors' browsers.

:::note
`--cors-enabled` lets a page call µQuery directly, but it allows every website, and visitors could send their own queries. Keep it for trusted networks.
:::

### The services

```yaml title="compose.yaml"
services:
  # Not published: only the web app can reach it.
  uquery:
    image: ${UQUERY_IMAGE:-docker.io/fb64/uquery:0.8}

  web:
    image: docker.io/denoland/deno:2.9.8
    command: run --no-lock --allow-net=0.0.0.0:8000,uquery:8080 --allow-read=public server.js
    working_dir: /app
    volumes:
      - .:/app:ro,z
    ports:
      - 127.0.0.1:8000:8000
    depends_on:
      - uquery
```

Only the `web` service has a published port. It runs on [Deno](https://deno.com/), which downloads [Hono](https://hono.dev/), the web framework, on first start: there's nothing to install or build. The `--allow-net` and `--allow-read` flags limit what the server can reach.

### The server

```js title="server.js"
import { Hono } from "jsr:@hono/hono@4.13.14";
import { serveStatic } from "jsr:@hono/hono@4.13.14/deno";

const UQUERY_URL = "http://uquery:8080";
const TAXI_FILE = "https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet";
const CO2_FILE = "https://raw.githubusercontent.com/owid/co2-data/master/owid-co2-data.csv";

// The SQL stays on the server: visitors can only call the routes below.
function query(sql) {
  return fetch(UQUERY_URL, {
    method: "POST",
    headers: { Accept: "application/json" },
    body: sql,
  });
}

const app = new Hono();

app.get("/api/taxi-trips", () =>
  query(`
    SELECT hour(tpep_pickup_datetime) AS hour, count(*) AS trips
    FROM '${TAXI_FILE}'
    GROUP BY hour ORDER BY hour`));

app.get("/api/countries", () =>
  query(`
    SELECT DISTINCT iso_code, country
    FROM '${CO2_FILE}'
    WHERE iso_code IS NOT NULL
    ORDER BY country`));

// The route only matches three capital letters, which makes the code safe to put in SQL.
app.get("/api/emissions/:code{[A-Z]{3}}", (c) =>
  query(`
    SELECT year, co2
    FROM '${CO2_FILE}'
    WHERE iso_code = '${c.req.param("code")}' AND co2 IS NOT NULL
    ORDER BY year`));

app.use("/*", serveStatic({ root: "./public" }));

Deno.serve(app.fetch);
```

Each route sends one query to µQuery and returns its response as it is. The emissions route takes a value from the visitor, so its pattern only accepts country codes. Always validate a value this way before you put it in SQL.

### The page

The [page](https://github.com/fb64/uquery-rs/tree/main/examples/web-app/public/index.html) is a single HTML file. It only knows the routes:

```js
async function get(path) {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.json();
}
```

```js
async function showEmissions(code) {
  const rows = await get(`/api/emissions/${code}`);
  draw("co2", "line", rows.map((r) => r.year), rows.map((r) => r.co2));
}
```

The charts are drawn with [Chart.js](https://www.chartjs.org/).

## Try a route

```bash
curl http://localhost:8000/api/taxi-trips
```

Expected output, with one object per hour:

```
[{"hour":0,"trips":79094},{"hour":1,"trips":53627},...]
```

## Stop the app

```bash
podman compose down
```

With Docker, run `docker compose down` instead.

## Larger results

JSON suits this app: the SQL aggregates the data on the server, so each result has a few hundred rows at most.

When a page needs many rows in the browser, send `Accept: application/vnd.apache.arrow.stream` in `query()` and read the result with [Apache Arrow JS](https://arrow.apache.org/js/). Arrow loads typed columns without parsing text, and [compressed](../response-formats.md#compression) sizes stay close to JSON. `BIGINT` columns such as `count(*)` arrive as JavaScript `BigInt`, so convert them with `Number()` before you chart them.

## Going public

Visitors can only call the routes of the server, so they can't run their own queries. Before you put an app like this online:

- Serve it over HTTPS, usually from your hosting platform or a reverse proxy.
- Check every value a route puts in SQL.
- Keep µQuery unpublished, and [set limits](../configuration.md) that match your data:

| Flag | Limits |
|---|---|
| `--allowed-directories`, `--allowed-urls` | What queries can read |
| `--query-timeout-secs`, `--max-query-duration-secs` | How long a query can run |
| `--memory-limit` | How much memory queries can use |

## Data sources

- [CO₂ and Greenhouse Gas Emissions](https://github.com/owid/co2-data) by Our World in Data, licensed under [CC BY](https://creativecommons.org/licenses/by/4.0/). Data that Our World in Data takes from third parties keeps its own terms.
- [TLC Trip Record Data](https://www.nyc.gov/site/tlc/about/tlc-trip-record-data.page) published by the NYC Taxi and Limousine Commission. The TLC states that it did not create the trip data and makes no representations as to its accuracy.

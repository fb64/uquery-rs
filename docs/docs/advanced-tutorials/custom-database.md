---
sidebar_position: 2
title: Custom Database
---

# Custom Database

µQuery allows you to attach a custom DuckDB database at startup. This is primarily used to expose pre-defined [DuckDB macros or macro tables](https://duckdb.org/docs/sql/statements/create_macro.html) so queries can reference them directly.

:::info
The attached database is opened in read-only mode.
:::

## Usage Example

### Get Data

For this example we'll use the New York City OpenData dataset of street-level temperature sensors.

```shell
curl "https://data.cityofnewyork.us/api/views/qdq3-9eqn/rows.csv?accessType=DOWNLOAD" --output /tmp/ny-temperature.csv
```

### Create custom database

```bash
# Create init script
echo "create macro nytemp() as table select * from read_csv('/tmp/ny-temperature.csv')" > init-db.sql
# Create DuckDB database
duckdb --no-stdin -init init-db.sql custom.db
```

### Start µQuery with custom database

#### With CLI

```bash
uquery -d custom.db

curl -X POST http://localhost:8080 \
  -H "Accept: text/csv" \
  -H "Content-Type: text/plain" \
  -d "select * from nytemp() limit 10"
```

Expected output:

```
Sensor.ID,AirTemp,Day,Hour,Latitude,Longitude,Year,Install.Type,Borough,ntacode
Bk-BR_01,71.189,2018-06-15,1,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,70.24333333,2018-06-15,2,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,69.39266667,2018-06-15,3,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
...
```

#### With Docker

Mount both the CSV file and the database file into the container:

```bash
docker run -p 8080:8080 \
  -v /tmp/ny-temperature.csv:/tmp/ny-temperature.csv \
  -v $(pwd)/custom.db:/custom.db \
  fb64/uquery -d /custom.db

curl -X POST http://localhost:8080 \
  -H "Accept: text/csv" \
  -H "Content-Type: text/plain" \
  -d "select * from nytemp() limit 10"
```

:::note
The CSV path inside the macro (`/tmp/ny-temperature.csv`) must match the path as seen inside the container.
:::

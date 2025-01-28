---
sidebar_position: 2
title: Custom Database
---

# Custom Database

µQuery allows you to embed a custom database that will be used for each query.
This feature is principally used to add pre-defined [DuckDB macro or macro table](https://duckdb.org/docs/sql/statements/create_macro.html) to make µQuery easy to use. 

:::info
Custom database is only used in read-only mode
:::

## Usage Example 

### Get Data

For this example we'll use the NewYork City OpenData about 

```shell
curl https://data.cityofnewyork.us/api/views/qdq3-9eqn/rows.csv\?accessType\=DOWNLOAD --output /tmp/ny-temperature.csv
```

### Create custom database

```bash
# Create init script
echo "create macro nytemp() as table select * from read_csv('/tmp/ny-temperature.csv')" > init-db.sql
# Create DuckDB Database
duckdb --no-stdin -init init-db.sql custom.db
```

### Start µQuery with custom database

#### With CLI

```bash
# Start µQuery
uquery -d custom.db
# Perform a query on the macro table
curl --location 'http://localhost:8080' \
--header 'Accept: text/csv' \
--header 'Content-Type: application/json' \
--data '{
    "query":"select * from nytemp() limit 10"
}'

# Results should be
Sensor.ID,AirTemp,Day,Hour,Latitude,Longitude,Year,Install.Type,Borough,ntacode
Bk-BR_01,71.189,2018-06-15,1,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,70.24333333,2018-06-15,2,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,69.39266667,2018-06-15,3,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,68.26316667,2018-06-15,4,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,67.114,2018-06-15,5,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,65.9655,2018-06-15,6,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,67.11433333,2018-06-15,7,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,68.89233333,2018-06-15,8,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,71.07416667,2018-06-15,9,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
Bk-BR_01,73.28616667,2018-06-15,10,40.66620508,-73.91691035,2018,Street Tree,Brooklyn,BK81
```

#### With Docker
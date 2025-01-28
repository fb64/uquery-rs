---
sidebar_position: 1
title: Query files
---

# Query files

DuckDB support querying multiple [file formats](https://duckdb.org/docs/guides/file_formats/overview). This example show how to use it to query parquet files.

[Apache parquet](https://parquet.apache.org/) is an efficient file format to store column-oriented data.

## Download Parquet files

Download [Yellow Taxi](https://www.nyc.gov/site/tlc/about/tlc-trip-record-data.page) record data 

```bash
wget https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet
``` 

## Run µQuery with files

```bash
docker run -p 8080:8080 -v ./yellow_tripdata_2024-01.parquet:/tmp/yellow_tripdata_2024-01.parquet fb64/uquery
```

## Query parquet files

```bash
curl --location 'http://localhost:8080' \
--header 'Accept: application/json' \
--header 'Content-Type: application/json' \
--data '{
    "query":"select * from read_parquet('\''/tmp/yellow_tripdata_2024-01.parquet'\'') limit 10"
}'
```

Note that [DuckDB HTTPFS extension](https://duckdb.org/docs/extensions/httpfs/overview.html) is preinstalled on docker image, so you can directly query file over http(s) 

```sql
select * from read_parquet('https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet') limit 10
```







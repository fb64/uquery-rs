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

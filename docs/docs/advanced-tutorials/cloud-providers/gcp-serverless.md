---
sidebar_position: 1
title: GCP Serverless
---

# Deploy µQuery on GCP Cloud Run

:::info Prerequisites

- A [Google Cloud](https://cloud.google.com) account with billing enabled
- [gcloud CLI](https://cloud.google.com/sdk/docs/install) installed and authenticated
- A GCP project with the following APIs enabled: Cloud Run, Cloud Storage, IAM

:::

This tutorial walks through deploying µQuery on [Cloud Run](https://cloud.google.com/run) to query files stored in [Cloud Storage](https://cloud.google.com/storage). Authentication is handled via the **GCP credential chain** — no API keys or HMAC secrets required.

---

## 1. Set variables

Define these once. All subsequent commands reference them.

```bash
export PROJECT_ID="your-project-id"
export REGION="europe-west1"
export BUCKET_NAME="uquery-data"
export SERVICE_ACCOUNT_NAME="uquery-sa"
export CLOUD_RUN_SERVICE="uquery"

# Derived
export SERVICE_ACCOUNT_EMAIL="${SERVICE_ACCOUNT_NAME}@${PROJECT_ID}.iam.gserviceaccount.com"

gcloud config set project $PROJECT_ID
```

---

## 2. Create a Cloud Storage bucket

```bash
gcloud storage buckets create gs://$BUCKET_NAME \
  --location=$REGION \
  --uniform-bucket-level-access
```

### Upload sample data

Download a [Yellow Taxi](https://www.nyc.gov/site/tlc/about/tlc-trip-record-data.page) Parquet file and upload it:

```bash
wget https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2026-01.parquet

gcloud storage cp yellow_tripdata_2026-01.parquet gs://$BUCKET_NAME/
```

---

## 3. Create a service account

```bash
gcloud iam service-accounts create $SERVICE_ACCOUNT_NAME \
  --display-name="uQuery Service Account" \
  --description="Service account for µQuery on Cloud Run"
```

### Grant read access to the bucket

```bash
gcloud storage buckets add-iam-policy-binding gs://$BUCKET_NAME \
  --member="serviceAccount:${SERVICE_ACCOUNT_EMAIL}" \
  --role="roles/storage.objectViewer"
```

:::tip Write access

If µQuery needs to write files (e.g. `COPY TO`), use `roles/storage.objectUser` instead.

:::

---

## 4. Deploy to Cloud Run

µQuery uses the **GCP credential chain** (`UQ_GCS_CREDENTIAL_CHAIN=true`) to authenticate against Cloud Storage. When running on Cloud Run, the attached service account is automatically detected — no secrets or key files needed.

```bash
gcloud run deploy $CLOUD_RUN_SERVICE \
  --image=docker.io/fb64/uquery:latest \
  --region=$REGION \
  --service-account=$SERVICE_ACCOUNT_EMAIL \
  --set-env-vars="UQ_GCS_CREDENTIAL_CHAIN=true" \
  --allow-unauthenticated \
  --port=8080 \
  --cpu=1 \
  --memory=512Mi \
  --min-instances=0 \
  --max-instances=10
```

Once deployed, retrieve the service URL:

```bash
export UQUERY_URL=$(gcloud run services describe $CLOUD_RUN_SERVICE \
  --region=$REGION \
  --format="value(status.url)")

echo $UQUERY_URL
```

---

## 5. Query your data

Use the `gcs://` prefix to reference Cloud Storage files directly in SQL.

### Count rows

```bash
curl -X POST $UQUERY_URL \
  -H "Content-Type: text/plain" \
  -d "SELECT COUNT(*) FROM 'gcs://$BUCKET_NAME/yellow_tripdata_2026-01.parquet'"
```

### Aggregate query

```bash
curl -X POST $UQUERY_URL \
  -H "Content-Type: text/plain" \
  -H "Accept: application/json" \
  -d "
    SELECT
      payment_type,
      COUNT(*)            AS trips,
      ROUND(AVG(fare_amount), 2) AS avg_fare
    FROM 'gcs://$BUCKET_NAME/yellow_tripdata_2026-01.parquet'
    GROUP BY payment_type
    ORDER BY trips DESC
  "
```

### Stream as Arrow IPC

```bash
curl -X POST $UQUERY_URL \
  -H "Content-Type: text/plain" \
  -H "Accept: application/vnd.apache.arrow.stream" \
  -d "SELECT * FROM 'gcs://$BUCKET_NAME/yellow_tripdata_2026-01.parquet' LIMIT 1000" \
  --output result.arrow
```

### Check service health

```bash
curl $UQUERY_URL/health
```

---

## 6. Restrict access (recommended for production)

By default the service is public (`--allow-unauthenticated`). To require a Google identity:

```bash
# Remove unauthenticated access
gcloud run services update $CLOUD_RUN_SERVICE \
  --region=$REGION \
  --no-allow-unauthenticated

# Call the service with an identity token
curl -X POST $UQUERY_URL \
  -H "Authorization: Bearer $(gcloud auth print-identity-token)" \
  -H "Content-Type: text/plain" \
  -d "SELECT 1"
```

---

## 7. Clean up

```bash
gcloud run services delete $CLOUD_RUN_SERVICE --region=$REGION
gcloud storage rm --recursive gs://$BUCKET_NAME
gcloud iam service-accounts delete $SERVICE_ACCOUNT_EMAIL
```

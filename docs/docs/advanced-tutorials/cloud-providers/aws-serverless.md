---
sidebar_position: 2
title: AWS Serverless
---

# Deploy µQuery on AWS Lambda

:::info Prerequisites

- An [AWS](https://aws.amazon.com) account
- [AWS CLI](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) installed and configured (`aws configure`)
- [Docker](https://docs.docker.com/get-docker/) installed

:::

This tutorial walks through deploying µQuery on [AWS Lambda](https://aws.amazon.com/lambda/) to query files stored in [S3](https://aws.amazon.com/s3/). Authentication is handled via the **AWS credential chain** — the Lambda execution role is automatically detected, no access keys required.

µQuery runs as a container on Lambda using the [AWS Lambda Web Adapter](https://github.com/awslabs/aws-lambda-web-adapter), which transparently proxies Lambda invocations to µQuery's HTTP server.

---

## 1. Set variables

Define these once. All subsequent commands reference them.

```bash
export AWS_REGION="us-east-1"
export ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
export BUCKET_NAME="uquery-data-${ACCOUNT_ID}"
export ECR_REPO="uquery"
export LAMBDA_FUNCTION="uquery"
export ROLE_NAME="uquery-lambda-role"

# Derived
export ECR_URI="${ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com/${ECR_REPO}"
export ROLE_ARN="arn:aws:iam::${ACCOUNT_ID}:role/${ROLE_NAME}"
```

---

## 2. Create an S3 bucket

```bash
aws s3api create-bucket \
  --bucket $BUCKET_NAME \
  --region $AWS_REGION \
  --create-bucket-configuration LocationConstraint=$AWS_REGION
```

:::note

Skip `--create-bucket-configuration` if your region is `us-east-1`.

:::

### Upload sample data

Download a [Yellow Taxi](https://www.nyc.gov/site/tlc/about/tlc-trip-record-data.page) Parquet file and upload it:

```bash
wget https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2026-01.parquet

aws s3 cp yellow_tripdata_2026-01.parquet s3://$BUCKET_NAME/
```

---

## 3. Create an IAM role for Lambda

### Trust policy

```bash
cat > trust-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": { "Service": "lambda.amazonaws.com" },
    "Action": "sts:AssumeRole"
  }]
}
EOF

aws iam create-role \
  --role-name $ROLE_NAME \
  --assume-role-policy-document file://trust-policy.json
```

### Attach execution policy

Allows Lambda to write logs to CloudWatch:

```bash
aws iam attach-role-policy \
  --role-name $ROLE_NAME \
  --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
```

### Grant S3 read access

```bash
cat > s3-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": ["s3:GetObject", "s3:ListBucket"],
    "Resource": [
      "arn:aws:s3:::${BUCKET_NAME}",
      "arn:aws:s3:::${BUCKET_NAME}/*"
    ]
  }]
}
EOF

aws iam put-role-policy \
  --role-name $ROLE_NAME \
  --policy-name uquery-s3-access \
  --policy-document file://s3-policy.json
```

---

## 4. Build and push the Lambda image

The [AWS Lambda Web Adapter](https://github.com/awslabs/aws-lambda-web-adapter) is added to the µQuery image so Lambda can proxy HTTP requests to it.

### Dockerfile

```dockerfile
FROM fb64/uquery:latest
COPY --from=public.ecr.aws/awsguru/aws-lambda-adapter:0.8.4 \
  /lambda-adapter /opt/extensions/lambda-adapter
```

### Build and push

```bash
# Create ECR repository
aws ecr create-repository --repository-name $ECR_REPO --region $AWS_REGION

# Authenticate Docker to ECR
aws ecr get-login-password --region $AWS_REGION \
  | docker login --username AWS --password-stdin \
    ${ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com

# Build, tag, and push
docker build -t $ECR_URI:latest .
docker push $ECR_URI:latest
```

---

## 5. Deploy the Lambda function

```bash
aws lambda create-function \
  --function-name $LAMBDA_FUNCTION \
  --package-type Image \
  --code ImageUri=$ECR_URI:latest \
  --role $ROLE_ARN \
  --environment Variables="{UQ_AWS_CREDENTIAL_CHAIN=true,PORT=8080}" \
  --timeout 30 \
  --memory-size 512 \
  --region $AWS_REGION
```

### Create a public Function URL

```bash
aws lambda create-function-url-config \
  --function-name $LAMBDA_FUNCTION \
  --auth-type NONE \
  --invoke-mode RESPONSE_STREAM \
  --region $AWS_REGION

export LAMBDA_URL=$(aws lambda get-function-url-config \
  --function-name $LAMBDA_FUNCTION \
  --region $AWS_REGION \
  --query FunctionUrl \
  --output text)

echo $LAMBDA_URL
```

---

## 6. Query your data

Use the `s3://` prefix to reference S3 files directly in SQL.

### Count rows

```bash
curl -X POST $LAMBDA_URL \
  -H "Content-Type: text/plain" \
  -d "SELECT COUNT(*) FROM 's3://$BUCKET_NAME/yellow_tripdata_2026-01.parquet'"
```

### Aggregate query

```bash
curl -X POST $LAMBDA_URL \
  -H "Content-Type: text/plain" \
  -H "Accept: application/json" \
  -d "
    SELECT
      payment_type,
      COUNT(*)                   AS trips,
      ROUND(AVG(fare_amount), 2) AS avg_fare
    FROM 's3://$BUCKET_NAME/yellow_tripdata_2026-01.parquet'
    GROUP BY payment_type
    ORDER BY trips DESC
  "
```

### Stream as Arrow IPC

```bash
curl -X POST $LAMBDA_URL \
  -H "Content-Type: text/plain" \
  -H "Accept: application/vnd.apache.arrow.stream" \
  -d "SELECT * FROM 's3://$BUCKET_NAME/yellow_tripdata_2026-01.parquet' LIMIT 1000" \
  --output result.arrow
```

### Check service health

```bash
curl $LAMBDA_URL/health
```

---

## 7. Restrict access (recommended for production)

By default the Function URL is public (`--auth-type NONE`). To require AWS IAM authentication:

```bash
aws lambda update-function-url-config \
  --function-name $LAMBDA_FUNCTION \
  --auth-type AWS_IAM \
  --region $AWS_REGION
```

Callers must then sign requests with [SigV4](https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-authenticating-requests.html). For quick testing with your own credentials:

```bash
curl -X POST $LAMBDA_URL \
  --aws-sigv4 "aws:amz:${AWS_REGION}:lambda" \
  --user "$(aws configure get aws_access_key_id):$(aws configure get aws_secret_access_key)" \
  -H "Content-Type: text/plain" \
  -d "SELECT 1"
```

---

## 8. Clean up

```bash
aws lambda delete-function --function-name $LAMBDA_FUNCTION --region $AWS_REGION
aws ecr delete-repository --repository-name $ECR_REPO --force --region $AWS_REGION
aws s3 rm s3://$BUCKET_NAME --recursive
aws s3api delete-bucket --bucket $BUCKET_NAME --region $AWS_REGION
aws iam detach-role-policy --role-name $ROLE_NAME \
  --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
aws iam delete-role-policy --role-name $ROLE_NAME --policy-name uquery-s3-access
aws iam delete-role --role-name $ROLE_NAME
rm trust-policy.json s3-policy.json
```

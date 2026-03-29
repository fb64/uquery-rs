---
sidebar_position: 5
title: Kubernetes
---

# Deploy µQuery on Kubernetes

This tutorial walks through deploying µQuery on Kubernetes with health checks, resource limits, and horizontal scaling.

:::info Prerequisites

- A running Kubernetes cluster
- `kubectl` configured to target it
- A container registry accessible from the cluster (Docker Hub, ECR, GCR, etc.)

:::

---

## 1. Deployment

The minimal deployment runs a single µQuery replica and exposes it on port 8080.

```yaml title="uquery-deployment.yaml"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: uquery
  labels:
    app: uquery
spec:
  replicas: 1
  selector:
    matchLabels:
      app: uquery
  template:
    metadata:
      labels:
        app: uquery
    spec:
      containers:
        - name: uquery
          image: fb64/uquery:latest
          ports:
            - containerPort: 8080
          env:
            - name: UQ_PORT
              value: "8080"
            - name: UQ_POOL_SIZE
              value: "4"
          resources:
            requests:
              cpu: 250m
              memory: 256Mi
            limits:
              cpu: "1"
              memory: 512Mi
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 3
            periodSeconds: 5
```

```bash
kubectl apply -f uquery-deployment.yaml
```

---

## 2. Service

Expose µQuery inside the cluster:

```yaml title="uquery-service.yaml"
apiVersion: v1
kind: Service
metadata:
  name: uquery
spec:
  selector:
    app: uquery
  ports:
    - port: 80
      targetPort: 8080
```

```bash
kubectl apply -f uquery-service.yaml
```

---

## 3. Ingress

Expose µQuery externally via an Ingress controller (e.g. nginx):

```yaml title="uquery-ingress.yaml"
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: uquery
  annotations:
    nginx.ingress.kubernetes.io/proxy-read-timeout: "300"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "300"
spec:
  rules:
    - host: uquery.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: uquery
                port:
                  number: 80
```

:::tip Timeouts

µQuery streams results incrementally. Set `proxy-read-timeout` and `proxy-send-timeout` high enough to accommodate large queries.

:::

```bash
kubectl apply -f uquery-ingress.yaml
```

---

## 4. Configuration

Use a `ConfigMap` to manage environment variables separately from the deployment:

```yaml title="uquery-configmap.yaml"
apiVersion: v1
kind: ConfigMap
metadata:
  name: uquery-config
data:
  UQ_PORT: "8080"
  UQ_POOL_SIZE: "4"
  UQ_QUERY_TIMEOUT: "30"
  UQ_CORS_ENABLED: "false"
```

Reference it in the deployment:

```yaml
          envFrom:
            - configMapRef:
                name: uquery-config
```

```bash
kubectl apply -f uquery-configmap.yaml
```

---

## 5. Cloud storage credentials

### AWS S3 (IRSA)

On EKS, use [IAM Roles for Service Accounts](https://docs.aws.amazon.com/eks/latest/userguide/iam-roles-for-service-accounts.html) to grant S3 access without static credentials:

```yaml
      serviceAccountName: uquery-sa  # annotated with the IAM role ARN
      containers:
        - name: uquery
          env:
            - name: UQ_AWS_CREDENTIAL_CHAIN
              value: "true"
```

### GCP Cloud Storage (Workload Identity)

On GKE, use [Workload Identity](https://cloud.google.com/kubernetes-engine/docs/how-to/workload-identity) to bind a Kubernetes service account to a GCP service account:

```yaml
      serviceAccountName: uquery-ksa  # bound to a GCP service account
      containers:
        - name: uquery
          env:
            - name: UQ_GCS_CREDENTIAL_CHAIN
              value: "true"
```

See the [GCP Serverless](./cloud-providers/gcp-serverless.md) and [AWS Serverless](./cloud-providers/aws-serverless.md) tutorials for the full IAM setup.

---

## 6. Horizontal scaling

Add a `HorizontalPodAutoscaler` to scale based on CPU usage:

```yaml title="uquery-hpa.yaml"
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: uquery
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: uquery
  minReplicas: 1
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
```

```bash
kubectl apply -f uquery-hpa.yaml
```

:::info

Each replica has its own independent connection pool (`UQ_POOL_SIZE`). Scaling adds replicas, not connections per replica.

:::

---

## 7. Verify the deployment

```bash
# Check pods are running
kubectl get pods -l app=uquery

# Check health
kubectl port-forward svc/uquery 8080:80
curl http://localhost:8080/health

# Run a query
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -d "SELECT 42 AS answer"
```

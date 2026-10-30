# Web app example

A web app charting public Parquet and CSV datasets, with a [Hono](https://hono.dev/) server in front of µQuery. The [Build a Web App](https://uquery.dev/docs/advanced-tutorials/web-app) tutorial explains how it works.

```bash
podman compose up    # or: docker compose up
```

Then open [http://localhost:8000](http://localhost:8000).

## Test a local build

Build an image from your binary and run the example with it, from the root of the repository:

```bash
cargo build --release
mkdir -p /tmp/uquery-image/bin
cp target/release/uquery /tmp/uquery-image/bin/uquery-amd64
podman build -t uquery:dev --build-arg TARGETARCH=amd64 -f Dockerfile /tmp/uquery-image
cd examples/web-app
UQUERY_IMAGE=uquery:dev podman compose up
```

With Docker, use `docker` in place of `podman`.

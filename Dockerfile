FROM rust:1.78-alpine3.20 as builder
RUN apk add --no-cache linux-headers g++ openssl-dev openssl-libs-static
WORKDIR /build
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release

FROM scratch
LABEL org.opencontainers.image.authors="florian@flob.fr"
LABEL org.opencontainers.image.source="https://github.com/fb64/uquery-rs"
LABEL org.opencontainers.image.description="A lightweight server that provide a simple API to query good old data files (CSV, Json, Parquet ...) with SQL"
EXPOSE 8080
COPY --from=builder /build/target/release/uquery .
ENTRYPOINT ["./uquery"]
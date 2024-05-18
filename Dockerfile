FROM rust:1.78 as builder
WORKDIR /build
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
EXPOSE 8080
COPY --from=builder /build/target/release/uquery /usr/local/bin/uquery
CMD ["uquery"]
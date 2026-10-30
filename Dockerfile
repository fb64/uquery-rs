# glibc and libstdc++ only: no shell, no package manager.
FROM gcr.io/distroless/cc-debian13@sha256:d74d144baa124dd9defd01d71872bb56c49d4441dba6153ccf790364f97e64b1
LABEL org.opencontainers.image.authors="dev@uquery.dev"

ARG TARGETARCH
COPY bin/uquery-${TARGETARCH} /usr/local/bin/uquery

USER 1000
ENV HOME=/home/uquery
WORKDIR /home/uquery
## Preload the extensions of the bundled DuckDB, in exec form as there is no shell
RUN ["/usr/local/bin/uquery", "--install-extensions"]

EXPOSE 8080
# Accept connections from outside the container (the binary defaults to 127.0.0.1)
ENV UQ_ADDR=0.0.0.0
WORKDIR /tmp
ENTRYPOINT ["uquery"]

FROM rust:1.75-bookworm AS builder

WORKDIR /app
COPY . .

RUN cargo build -p rustdesk-server-console --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rustdesk-server-console /usr/local/bin/
COPY --from=builder /app/server/src/static /opt/rustdesk-console/static

ENV STATIC_DIR=/opt/rustdesk-console/static
ENV LISTEN_ADDR=0.0.0.0:21114

EXPOSE 21114

CMD ["rustdesk-server-console"]

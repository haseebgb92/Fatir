FROM rust:1.84-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/fatir-relay /usr/local/bin/fatir-relay
ENV PORT=8787
EXPOSE 8787
CMD ["fatir-relay"]

FROM rust:1-bookworm AS build
WORKDIR /workspace
COPY lib/arena-server ./lib/arena-server
RUN cargo build --release --locked --manifest-path lib/arena-server/Cargo.toml

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /workspace/lib/arena-server/target/release/server /usr/local/bin/server
COPY assets/worlds.json /app/assets/worlds.json
ENV HOST=0.0.0.0 PORT=8080 DATS_WORLDS_PATH=/app/assets/worlds.json
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/server"]

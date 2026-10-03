FROM rust:1-bookworm AS rust-build
WORKDIR /workspace
COPY lib/arena-server ./lib/arena-server
RUN cargo build --release --locked --manifest-path lib/arena-server/Cargo.toml

FROM python:3.12-slim
WORKDIR /app
COPY --from=rust-build /workspace/lib/arena-server/target/release/server /usr/local/bin/server
COPY modules/arena-runtime/runtime.py /app/modules/arena-runtime/runtime.py
COPY assets/worlds.json /app/assets/worlds.json
ENV PYTHONUNBUFFERED=1 DATS_ARENA_BIN=/usr/local/bin/server DATS_WORLDS_PATH=/app/assets/worlds.json
EXPOSE 8080 9001
CMD ["python3", "/app/modules/arena-runtime/runtime.py"]

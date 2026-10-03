FROM python:3.12-slim
WORKDIR /app
COPY modules/arena-hub /app/modules/arena-hub
COPY docs/components/arena-hub /app/docs/components/arena-hub
COPY docs/mechanics.md /app/docs/mechanics.md
COPY assets/worlds.json /app/assets/worlds.json
RUN mkdir -p /app/data /app/lib/arena-bots/rust_bytadaniel
ENV PYTHONUNBUFFERED=1
CMD ["python3", "/app/modules/arena-hub/hub.py"]

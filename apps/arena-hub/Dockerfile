FROM python:3.12-slim
WORKDIR /app
COPY apps/arena-hub /app/apps/arena-hub
COPY docs/components/arena-hub /app/docs/components/arena-hub
COPY docs/mechanics.md /app/docs/mechanics.md
COPY assets/worlds.json /app/assets/worlds.json
RUN mkdir -p /app/data /app/lib/bot-variants/player_2
ENV PYTHONUNBUFFERED=1
CMD ["python3", "/app/apps/arena-hub/hub.py"]

#!/usr/bin/env bash
set -e

# Запуск графического клиента визуализации DatsMagic
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VIS_DIR="$ROOT_DIR/visualizer"
VENV_DIR="$VIS_DIR/.venv"

echo "=== Запуск визуализатора DatsMagic ==="

# Проверяем или создаем venv
if [ ! -d "$VENV_DIR" ]; then
    echo "Создание виртуального окружения в $VENV_DIR..."
    python3 -m venv "$VENV_DIR"
    echo "Установка зависимостей из requirements.txt..."
    "$VENV_DIR/bin/pip" install --upgrade pip
    "$VENV_DIR/bin/pip" install -r "$VIS_DIR/requirements.txt"
fi

# Убедимся, что пакет доступен
export PYTHONPATH="$VIS_DIR:$PYTHONPATH"
exec "$VENV_DIR/bin/python" -m visualizer.main "$@"


#!/bin/sh
set -e

CONFIG_PATH="${MYSQL_EXECUTOR_CONFIG:-/app/config/config.toml}"
DEFAULT_CONFIG="/app/default-config.toml"

if [ -f "$CONFIG_PATH" ]; then
    echo "[INFO] 使用用户配置: $CONFIG_PATH"
else
    echo "[INFO] 未找到用户配置，使用 Docker 默认配置"
    # 确保目录存在
    mkdir -p "$(dirname "$CONFIG_PATH")"
    cp "$DEFAULT_CONFIG" "$CONFIG_PATH"
fi

exec /app/mysql-executor "$@"

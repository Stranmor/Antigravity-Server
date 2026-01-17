# Antigravity WARP Isolation

IP изоляция для Google аккаунтов через Cloudflare WARP.

## Архитектура

```
┌─────────────────────────────────────────────────────────────────┐
│                    Antigravity Proxy Server                      │
│                         (port 8045)                              │
├─────────────────────────────────────────────────────────────────┤
│                      IP Guard Module                             │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐           │
│   │Account 1│  │Account 2│  │Account 3│  │Account N│           │
│   │warp-aaa │  │warp-bbb │  │warp-ccc │  │warp-xxx │           │
│   │:10800   │  │:10801   │  │:10802   │  │:108xx   │           │
│   └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘           │
├────────┼────────────┼───────────┼────────────┼──────────────────┤
│        │            │           │            │                   │
│   ┌────▼────┐  ┌────▼────┐  ┌───▼────┐  ┌───▼────┐             │
│   │Gluetun  │  │Gluetun  │  │Gluetun │  │Gluetun │   Containers│
│   │WARP VPN │  │WARP VPN │  │WARP VPN│  │WARP VPN│             │
│   └────┬────┘  └────┬────┘  └───┬────┘  └───┬────┘             │
│        │            │           │            │                   │
└────────┼────────────┼───────────┼────────────┼──────────────────┘
         │            │           │            │
    ┌────▼────┐  ┌────▼────┐  ┌───▼────┐  ┌───▼────┐
    │IP: A.B.C│  │IP: D.E.F│  │IP: G.H.I│  │IP: X.Y.Z│   Unique IPs
    └─────────┘  └─────────┘  └─────────┘  └─────────┘
```

## Компоненты

### 1. generate-warp-containers.sh

Генерирует systemd quadlet файлы для каждого аккаунта:
- Читает `/var/lib/antigravity/accounts.json`
- Создаёт WireGuard ключи для каждого аккаунта
- Генерирует Gluetun контейнеры с WARP VPN
- Назначает уникальные SOCKS5 порты (10800+)

```bash
./generate-warp-containers.sh \
    --accounts-file /var/lib/antigravity/accounts.json \
    --quadlet-dir /etc/containers/systemd \
    --base-port 10800
```

### 2. verify-warp-ips.sh

Проверяет уникальность IP-адресов:
- Получает внешний IP через каждый SOCKS5 прокси
- Детектирует коллизии (один IP на несколько аккаунтов)
- Детектирует bypass (IP прокси = IP хоста)
- Выводит JSON отчёт

```bash
./verify-warp-ips.sh --mapping-file /etc/antigravity/warp/ip_mapping.json
```

### 3. deploy-warp-isolation.sh

Полный деплой на VPS:
- Устанавливает зависимости (wireguard-tools, jq)
- Деплоит скрипты
- Генерирует контейнеры
- Запускает и верифицирует

```bash
./deploy-warp-isolation.sh vps-production
```

## Структура файлов на VPS

```
/etc/antigravity/warp/
├── ip_mapping.json          # Маппинг аккаунт → порт → контейнер
├── {account_id}/
│   ├── private.key          # WireGuard приватный ключ (600)
│   ├── public.key           # WireGuard публичный ключ
│   └── config.json          # Метаданные

/etc/containers/systemd/
├── antigravity-warp.network # Общая bridge-сеть
├── warp-xxxxxxxx.container  # Quadlet для аккаунта 1
├── warp-yyyyyyyy.container  # Quadlet для аккаунта 2
└── ...
```

## Интеграция с Antigravity Proxy

### ip_mapping.json

```json
{
  "version": "1.0",
  "generated_at": "2026-01-17T12:00:00Z",
  "accounts": [
    {
      "id": "abc123...",
      "email": "user1@gmail.com",
      "warp_port": 10800,
      "warp_container": "warp-abc123",
      "socks5_endpoint": "socks5://127.0.0.1:10800"
    },
    {
      "id": "def456...",
      "email": "user2@gmail.com", 
      "warp_port": 10801,
      "warp_container": "warp-def456",
      "socks5_endpoint": "socks5://127.0.0.1:10801"
    }
  ]
}
```

### Конфигурация прокси

Добавить в `/etc/antigravity/env`:

```bash
# WARP IP Isolation
WARP_ENABLED=true
WARP_MAPPING_FILE=/etc/antigravity/warp/ip_mapping.json
```

## Безопасность

### Проверки IP Guard

1. **Collision Detection**: Если два аккаунта получили одинаковый IP → ошибка
2. **Bypass Detection**: Если IP через прокси = IP хоста → ошибка VPN
3. **Pre-flight Check**: Проверка перед каждым запросом к Google API

### Защита ключей

- Приватные ключи: `chmod 600`
- Директория: `chmod 700`
- Podman secrets для передачи в контейнеры

## Мониторинг

### Проверка состояния

```bash
# Статус контейнеров
podman ps --filter 'name=warp-'

# Логи конкретного контейнера
podman logs -f warp-abc123

# Верификация IP
/opt/antigravity/warp-isolation/verify-warp-ips.sh

# Проверка конкретного прокси
curl --socks5 127.0.0.1:10800 https://api.ipify.org
```

### Cron-проверка

```bash
# /etc/cron.d/antigravity-warp-verify
0 */6 * * * root /opt/antigravity/warp-isolation/verify-warp-ips.sh --json --output /var/log/antigravity/warp-verify.json
```

## Troubleshooting

### Контейнер не запускается

```bash
# Проверить логи
journalctl -u warp-xxxxxxxx.service

# Проверить секрет
podman secret ls | grep warp-

# Пересоздать секрет
podman secret rm warp-xxxxxxxx-privkey
podman secret create warp-xxxxxxxx-privkey /etc/antigravity/warp/{account_id}/private.key
```

### IP collision

```bash
# Регенерировать ключи для аккаунта
rm -rf /etc/antigravity/warp/{account_id}
./generate-warp-containers.sh  # Создаст новые ключи

# Перезапустить контейнер
systemctl restart warp-xxxxxxxx.service
```

### VPN не подключается

```bash
# Проверить endpoint
curl -v https://162.159.192.1:2408 --connect-timeout 5

# Попробовать альтернативный endpoint
# В quadlet изменить VPN_ENDPOINT_IP на:
#   162.159.192.5
#   162.159.193.1
#   engage.cloudflareclient.com
```

## Ограничения

- Cloudflare WARP Free: ~100Mbps, shared IP pool
- Для dedicated IP нужен WARP+ или WARP Enterprise
- Один WireGuard ключ → один IP (может меняться при реконнекте)
- Максимум ~50-100 контейнеров на VPS (зависит от RAM)

## TODO

- [ ] Интеграция IP Guard в antigravity-core
- [ ] Автоматическая ротация при детекции бана
- [ ] Dashboard для мониторинга IP-статуса
- [ ] WARP+ лицензии для стабильных IP

# Stock Quote Streaming System

Сервер и клиент для бинарного UDP стрима котировок

## Че делает

**Server** (`quote_server`) - генерирует данные Quote и рассылает по клиентам:
- Клиент подключается по TCP
- По `STREAM` команде и переданным UDP сокет адресу и списку тикеров регистрирует клиента 
- Делает Fan Out по всем клиентам
- Если клиент не заслалает `PING` с заданой регулярностью, отключает клиента

**Client** (`quote_client`) - подключается к серверу и получает данные стонксов:
- Клиент подключается по TCP и шлет `STREAM` команду
- Получает `bincode`-кодированный стонк квот `StockQuote` по UDP
- Переодически делает `PING` чтобы сервер не перестал слать сообщения
- Выводит данные по тикерам

## tl;dr

```
Client → Server (TCP):   STREAM udp://127.0.0.1:6667 AAPL,MSFT,TSLA
Server → Client (TCP):   OK
Server → Client (UDP):   <bincode StockQuote>
Client → Server (UDP):   PING
Client → Server (TCP):   EXIT
```

## схема потоков

```mermaid
flowchart TD
    subgraph SRV["Server Process"]
        subgraph GEN_T["Generator Thread"]
            GEN["QuoteGenerator\nrandom walk · tick every N ms"]
        end

        subgraph MAIN_T["QuotesSender Thread  (crossbeam select!)"]
            SEL["select!\ncmd_receiver · quote_receiver · timeout"]
            RETAIN["retain:\ndrop subscriptions with stale last_ping"]
            FANOUT["fan-out:\nClientStreamMessage to per-client channels"]
        end

        subgraph PING_T["Ping Receiver Thread"]
            PING_RX["recv_from UDP\nfilter PING → Command::Ping"]
        end

        subgraph TCP_T["TCP Handler Threads\none per connection"]
            TCP_H["parse STREAM / EXIT\nreply OK / ERR"]
        end

        subgraph HC_T["Per-Client Threads\none per active subscription"]
            HC["handle_client\nbincode serialize → send_to"]
        end

        GEN -- "Vec<StockQuote>\nunbounded channel" --> SEL
        TCP_H -- "Command::Stream\nunbounded channel" --> SEL
        PING_RX -- "Command::Ping\nunbounded channel" --> SEL
        SEL --> RETAIN
        SEL --> FANOUT
        FANOUT -- "ClientStreamMessage\nper-client unbounded channel" --> HC
    end

    subgraph CLI["Client Process"]
        TCP_C["main thread\nTCP connect · STREAM command"]
        RECV["receive_loop\nUDP recv → decode → output"]
        PING_C["ping_loop thread\nUDP PING every ping-delay ms"]
    end

    TCP_C -- "TCP: STREAM udp://host:port tickers" --> TCP_H
    HC -- "UDP: bincode StockQuote" --> RECV
    PING_C -- "UDP: PING" --> PING_RX
```

## Как включить

```bash
# Server - дефолтные настройки: TCP :8971, UDP :8988, tickers from server/cfg/tickers
cargo run -p server

# Server - кастомные настройки
cargo run -p server -- \
  --tcp-port 8971 \
  --udp-port 8988 \
  --ticker-list server/cfg/tickers \
  --delay-ms 2000 \
  --ping-cooldown-ms 5000 \
  --tick-duration-ms 1000 \
  --price-deviation 5 \
  --capacity 100

# Client - дефолтные настройки: server 127.0.0.1:8971, UDP callback :6667
cargo run -p client

# Client - кастомные настройки
cargo run -p client -- \
  --server-addr 127.0.0.1:8971 \
  --udp-uri 127.0.0.1:8988 \
  --udp-port 6667 \
  --tickers-file client/cfg/tickers \
  --ping-delay 1000 \
  --output-file quotes.log   # можно не задавать, чтобы выводило в stdout
```

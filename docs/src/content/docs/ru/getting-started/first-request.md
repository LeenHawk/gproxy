---
title: "Первый запрос"
description: "Вызовите шлюз через API-интерфейсы OpenAI, Claude и Gemini, используйте потоковую передачу и диагностируйте распространенные ошибки."
---

После [Быстрый старт](/ru/getting-started/quick-start/) у вас должен быть маршрут модели с именем `fast` и ключ API шлюза. В этих примерах используется адрес по умолчанию; отрегулируйте его, если вы изменили порт.

```sh
export GPROXY_KEY='your-gproxy-api-key'
```

Шлюз принимает `Authorization: Bearer <key>`, `x-api-key` и `x-goog-api-key`. Используйте ключ шлюза, а не учётные данные провайдера.

<a id="openai-chat-completions"></a>

## OpenAI Завершения чата

```sh
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

<a id="openai-responses"></a>

## OpenAI Ответы

```sh
curl -sS http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","input":"Say hello."}'
```

<a id="claude-messages"></a>

## Claude Сообщения

```sh
curl -sS http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: $GPROXY_KEY" \
  -H 'anthropic-version: 2023-06-01' \
  -H 'content-type: application/json' \
  -d '{"model":"fast","max_tokens":256,
       "messages":[{"role":"user","content":"Say hello."}]}'
```

<a id="gemini-generatecontent"></a>

## Gemini Генерировать контент

```sh
curl -sS "http://127.0.0.1:8787/v1beta/models/fast:generateContent" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Say hello."}]}]}'
```

<a id="streaming"></a>

## Потоковое вещание

```sh
curl -sSN http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","stream":true,
       "messages":[{"role":"user","content":"Count to three."}]}'
```

<a id="gemini-streaming"></a>

## Gemini потоковое вещание

```sh
curl -sSN "http://127.0.0.1:8787/v1beta/models/fast:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Count to three."}]}]}'
```

Ответы используют протокол, запрошенный клиентом. Успех зависит от участников маршрута, возможностей провайдера и поддержки преобразования. Изменение конечной точки не добавляет провайдеру возможностей.

<a id="model-lists"></a>

## Списки моделей

Используйте каталог моделей консоли, чтобы найти настроенные имена шлюзов. Чтобы запросить каталог API, видимый клиенту шлюза, через API:

```sh
curl -sS http://127.0.0.1:8787/portal/api/models \
  -H "Authorization: Bearer $GPROXY_KEY"
```

`permitted` указывает, может ли клиент API использовать запись. `GET /v1/models` пересылает список моделей выбранного провайдера, который отличается от каталога шлюзов. Приложение не предоставляет `/portal/api` поверх HTTP; используйте каталог в приложении.

<a id="select-a-provider-or-namespace"></a>

## Выберите провайдера или пространство имен

| URL | Модель в запросе | Цель |
| --- | --- | --- |
| `/v1/chat/completions` | `fast` | Маршрут `fast` |
| `/openai-main/v1/chat/completions` | модель провайдера ID | Провайдер `openai-main` |
| `/acme/v1/chat/completions` | `fast` | Маршрут `acme/fast` |

См. [Модели и маршруты](/ru/guides/models/) для правил именования.

<a id="common-errors"></a>

## Распространенные ошибки

| Статус | Значение |
| --- | --- |
| `400` | Неверный запрос или значение конфигурации |
| `401` | Ключ отсутствует, недействителен, отключен или срок его действия истек |
| `403` | Недостаточно разрешений или области действия OAuth |
| `404` | Неизвестная модель, маршрут или ресурс |
| `429` | Применяются ограничения по ставке или квоте |
| `5xx` | Сбой шлюза или провайдера; проверить журналы |

После запроса проверьте использование и записи запросов в консоли. Сохранение тела зависит от настроек ведения журнала; см. [Использование, журналы и аудит](/ru/guides/observability/).

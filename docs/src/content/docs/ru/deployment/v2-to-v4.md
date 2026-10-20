---
title: Миграция с v2 на v4
description: Единый скрипт миграции с v2 на v4 для SQLite, PostgreSQL и MySQL.
---

Используйте [migrate-v2-to-v4.py](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-to-v4.py). Источником и целью могут быть SQLite, PostgreSQL или MySQL, в том числе разные СУБД.

Один запуск выполняет цепочку **источник v2 → снимок SQLite → v3 SQLite → v4 SQLite → целевая база**. Источник открывается только для чтения. Существующие средства обновления преобразуют аккаунты, учётные данные, ключи API и использование, после чего полные данные v4 копируются в целевую базу.

## Подготовка

Остановите старый экземпляр и сохраните резервную копию. Цель должна быть **новым файлом SQLite или пустой базой PostgreSQL/MySQL**, а не исходной базой. Для зашифрованного экземпляра сохраните прежний `GPROXY_MASTER_KEY`.

Подготовьте Python 3.10+, исполняемые файлы [v3.0.22](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22) и [v4](https://github.com/LeenHawk/gproxy/releases/latest). Сборка v4 должна включать драйвер целевой СУБД. Для SQLite дополнительные пакеты Python не нужны; для PostgreSQL или MySQL установите соответствующий драйвер:

```sh
python3 -m pip install 'psycopg[binary]>=3,<4'  # PostgreSQL
python3 -m pip install 'PyMySQL[rsa]>=1,<2'     # MySQL
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-to-v4.py -o migrate-v2-to-v4.py
```

## Запуск миграции

Пример для SQLite:

```sh
python3 migrate-v2-to-v4.py \
  --source ./old-data/gproxy.db \
  --target ./new-data/gproxy.db \
  --v3-bin ./v3/gproxy \
  --v4-bin ./v4/gproxy
```

Для удалённой базы замените любой из аргументов базы на `postgresql://user:password@host/database` или `mysql://user:password@host/database`. Чтобы пароль не попадал в аргументы процесса, задайте `GPROXY_SOURCE_DSN` и `GPROXY_TARGET_DSN` и опустите соответствующие флаги. При необходимости передайте прежний мастер-ключ через окружение:

```sh
read -rsp 'DSN исходной базы: ' GPROXY_SOURCE_DSN; echo; export GPROXY_SOURCE_DSN
read -rsp 'DSN пустой целевой базы: ' GPROXY_TARGET_DSN; echo; export GPROXY_TARGET_DSN
read -rsp 'Прежний мастер-ключ (пусто без шифрования): ' GPROXY_MASTER_KEY; echo; export GPROXY_MASTER_KEY
python3 migrate-v2-to-v4.py --v3-bin ./v3/gproxy --v4-bin ./v4/gproxy
```

Скрипт создаёт закрытый рабочий каталог и показывает его путь; `--work-dir` позволяет указать новый каталог. По умолчанию исполняемые файлы ищутся в PATH как `gproxy-v3` и `gproxy`. Для источника PostgreSQL вне `public` используйте `--source-schema`; целевая база PostgreSQL использует `public`. Таблицы источника MySQL должны использовать InnoDB. Параметры DSN `ssl-mode`, `ssl-ca`, `ssl-cert` и `ssl-key` задают TLS для MySQL.

## После завершения

Промежуточные базы и отчёты остаются в рабочем каталоге. Проверяются аккаунты, хэши ключей API и исторические расходы; при копировании в PostgreSQL/MySQL также сверяются количество и контрольные суммы строк каждой таблицы данных. Изучите `v3-import.log`, `v4-import.log` и `*-verification.json`, затем запустите v4 с целевой базой и прежним мастер-ключом.

Ограничения существующих преобразований сохраняются: часть старых разрешений и правил нужно настроить заново. Оставшиеся подробные записи использования переносятся, но удалённые нельзя восстановить из агрегатов. Старые журналы запросов, сеансы, аудит и кэш не превращаются в соответствующие записи v4. v4 хранит суммы с 9 десятичными знаками, а точные исторические значения — в `metrics.v3.cost`. См. [v2 → v3](/ru/deployment/v2-to-v3/) и [v3 → v4](/ru/deployment/v3-to-v4/).

При ошибке скрипт останавливается и сохраняет рабочие файлы; источник не удаляется, цель автоматически не очищается. После диагностики используйте новый рабочий каталог и пустую целевую базу. Каталог содержит секреты и должен оставаться закрытым. v3.0.22 кратковременно получает ключ через аргументы процесса; не включайте трассировку shell.

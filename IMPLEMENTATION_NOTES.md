# Реализовано в Team Edition

## Desktop-приложение

- Новый тёмно-красный интерфейс с вкладками: Главная, История, Diff, Backups, Серверы, Настройки, AI, Профиль.
- Вход через Telegram-код: приложение отправляет код на backend, сохраняет JWT и получает профиль/роль/лимиты.
- История последних 25 запусков берётся с backend, а если backend недоступен — из локальной истории.
- Серверы и множественные UK/PDD-ссылки загружаются с backend и синхронизируются в локальный Tauri-конфиг перед запуском парсера.
- Поддержка любого количества ссылок на один сервер через `forum_uk_urls` и `forum_pdd_urls`.
- Старые поля `forum_uk_url`, `forum_uk_url_2`, `forum_pdd_url`, `forum_pdd_url_2` сохранены для совместимости со старым config.json.
- Если ломается хотя бы одна ссылка из выбранной группы UK/PDD, весь режим останавливается.
- OpenAI больше не требует локальный ключ в приложении: клиент вызывает backend endpoint `/ai/openai/generate` с JWT.
- Gemini остаётся локальным: пользователь вводит свой Gemini API key, лимиты не списываются.
- Diff-before-save: `run_update` теперь создаёт pending JSON, показывает diff, а реальная запись файла происходит только после кнопки “Сохранить”.
- Кнопка “Отмена” на Diff-экране удаляет pending-файл и пишет отменённый запуск в историю.
- Backup создаётся только при подтверждённом сохранении: `.backup.json` и timestamp-copy в папке `backups/`.
- CSP больше не `null`; разрешены только нужные источники: Tauri IPC, localhost/backend, GitHub raw, Gemini.
- AppImage убран из сборки; Linux release собирает только `.deb`.

## Backend

- FastAPI backend с JWT-защитой рабочих endpoints.
- Bootstrap первого админа через `.env` переменную `ADMIN_TELEGRAM_IDS`.
- Роли: `admin`, `editor`, `user`.
- Таблицы: users, servers, server_links, check_runs, daily_usage, login_codes, incidents.
- Серверы можно добавлять, переименовывать и удалять.
- На сервер можно добавлять любое количество UK/PDD-ссылок.
- Лимит списывается только при `status=success` и `file_saved=true`.
- Gemini не списывает лимиты.
- История отдаёт последние 25 записей.
- Инциденты создаются через API и отправляются всем admin/editor в Telegram личку.
- OpenAI endpoint хранит ключ только на backend через `OPENAI_API_KEY`.

## Telegram-бот

Команды:

```text
/start
/help
/me
/users
/add_user telegram_id role limit [username]
/set_role telegram_id admin|editor|user
/set_limit telegram_id число
/ban_user telegram_id
/unban_user telegram_id
/servers
/add_server Название
/rename_server server_id Новое название
/delete_server server_id
/add_link server_id UK|PDD Название | https://url
/edit_link link_id UK|PDD Название | https://url
/delete_link link_id
/check_link https://url
/history
/incidents
```

Editor/Admin могут управлять серверами и ссылками. Только Admin может управлять пользователями, ролями и лимитами.

## Проверки, которые я смог выполнить в этой среде

```bash
python3 -m compileall -q backend/app
cd tauri-app && npm install && npm run build
```

Обе проверки проходят.

`cargo check`/`cargo build` не запущены, потому что в текущей среде нет Rust/Cargo.

## Что ещё рекомендуется для production

- Прогнать `cargo check`, `cargo test` и `npm run tauri build` на машине/CI с Rust toolchain.
- Добавить миграции Alembic вместо `metadata.create_all`, если база уже будет использоваться в продакшене.
- Подключить Tauri updater с приватным update manifest и подписью обновлений.
- По возможности убрать Linux fallback `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1`, если WebKit стабильно работает без него на целевых машинах.
- При желании заменить текстовые команды Telegram-бота на inline-кнопки; бизнес-логика уже есть.

## Важное ограничение текущей desktop-логики

Из-за ручного diff-before-save приложение запускает проверку только для одного сервера за раз. Режим `UK+PDD` поддерживается: создаются два pending JSON, показывается общий diff, а backend списывает один лимит после подтверждения сохранения.

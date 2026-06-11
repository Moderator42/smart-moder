# Smart Config Team Backend

Backend для приватной команды Smart Config Editor.

## Возможности

- Telegram-login через одноразовый код из бота.
- JWT-защита API.
- Роли: `admin`, `editor`, `user`.
- Дневной лимит по времени сервера.
- OpenAI ключ хранится только на backend.
- Gemini работает локально в desktop-приложении и не списывает лимиты.
- Серверы и любое количество UK/PDD-ссылок.
- История последних 25 запусков.
- Инциденты парсинга с уведомлением всем admin/editor в личку.

## Быстрый запуск

```bash
cd backend
cp .env.example .env
# отредактируй .env, обязательно ADMIN_TELEGRAM_IDS и TELEGRAM_BOT_TOKEN
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
uvicorn app.main:app --reload
```

В отдельном терминале:

```bash
cd backend
source .venv/bin/activate
python -m app.bot
```

## Первый админ

В `.env` укажи свой Telegram ID:

```env
ADMIN_TELEGRAM_IDS=123456789
```

При старте backend этот пользователь будет создан/обновлён как `admin`.

## Основная логика лимитов

- UK отдельно = 1 лимит после успешного сохранения.
- PDD отдельно = 1 лимит после успешного сохранения.
- UK+PDD одним запуском в интерфейсе считается как один сценарий: desktop создаёт pending для UK и PDD, показывает общий diff, а backend списывает один лимит только при `/runs/finish` с `success + file_saved=true`.
- Gemini не списывает лимит.
- Ошибка парсинга / сломанная ссылка / отмена на diff-экране не списывает лимит.

## Команды Telegram-бота

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

## Запуск через Docker для Postgres/Redis

Из корня проекта:

```bash
docker compose up -d postgres redis
```

Потом запусти API и бота обычными командами выше.

## Важное ограничение текущей desktop-логики

Из-за ручного diff-before-save приложение запускает проверку только для одного сервера за раз. Режим `UK+PDD` поддерживается: создаются два pending JSON, показывается общий diff, а backend списывает один лимит после подтверждения сохранения.

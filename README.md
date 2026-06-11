# Smart Config Editor — Team Edition

Tauri + Svelte desktop-приложение и FastAPI backend для приватной команды, которая обновляет SmartUK/SmartPDD JSON через AI.

## Что есть

- Telegram-авторизация через код из бота.
- Роли `admin`, `editor`, `user`.
- Лимит проверок в день на пользователя.
- Серверы и любое количество UK/PDD-ссылок на один сервер.
- Если одна ссылка сломалась — весь режим UK/PDD останавливается, JSON не создаётся, лимит не списывается.
- OpenAI ключ хранится только на backend.
- Gemini ключ хранится локально у пользователя, лимиты не списываются.
- Diff-before-save: файл сохраняется только после подтверждения на Diff-экране.
- Backup перед каждым подтверждённым сохранением.
- История последних 25 запусков.
- Инциденты парсинга отправляются admin/editor в Telegram личку.
- Linux сборка только `.deb`; AppImage отключён.
- Arch установка через локальную конвертацию `.deb` с помощью `debtap`.

## Backend

```bash
cd backend
cp .env.example .env
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

В `.env` обязательно укажи:

```env
DATABASE_URL=postgresql+asyncpg://smart:smart@localhost:5432/smart_config
JWT_SECRET=change-me
TELEGRAM_BOT_TOKEN=123456:token
OPENAI_API_KEY=sk-your-server-key
ADMIN_TELEGRAM_IDS=123456789
```

## Desktop development

```bash
cd tauri-app
npm install
npm run tauri dev
```

## Проверки

```bash
python3 -m compileall -q backend/app
cd tauri-app
npm install
npm run build
cd src-tauri
cargo test
```

## Build `.deb`

```bash
cd tauri-app
npm run build:deb
```

Debian-пакет появится здесь:

```text
tauri-app/src-tauri/target/release/bundle/deb/
```

## Arch install через debtap

```bash
yay -S debtap
sudo debtap -u
sudo debtap -q Smart.Config.Editor_0.1.0_amd64.deb
sudo pacman -U smart-config-editor-*.pkg.tar.zst
```

## Packaging

- Linux CI produces only `.deb`.
- AppImage intentionally disabled because Ubuntu-built AppImage can conflict with Arch rolling libraries.
- Windows CI produces NSIS/MSI installers.

## Важное ограничение текущей desktop-логики

Из-за ручного diff-before-save приложение запускает проверку только для одного сервера за раз. Режим `UK+PDD` поддерживается: создаются два pending JSON, показывается общий diff, а backend списывает один лимит после подтверждения сохранения.

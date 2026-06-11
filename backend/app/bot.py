import asyncio
import secrets
from datetime import datetime, timedelta

from aiogram import Bot, Dispatcher
from aiogram.filters import Command
from aiogram.types import Message
from sqlalchemy import select
from sqlalchemy.orm import selectinload

from .db import SessionLocal, init_db
from .models import LoginCode, Server, ServerLink, User, CheckRun, Incident
from .services import VALID_ROLES, validate_forum_url
from .settings import settings

bot = Bot(settings.telegram_bot_token)
dp = Dispatcher()


def role_ok(user: User | None, roles: set[str]) -> bool:
    return bool(user and user.is_active and user.role in roles)


def is_admin(user: User | None) -> bool:
    return role_ok(user, {"admin"})


def is_editor(user: User | None) -> bool:
    return role_ok(user, {"admin", "editor"})


async def get_actor(session, telegram_id: int) -> User | None:
    return await session.scalar(select(User).where(User.telegram_id == telegram_id))


def help_text(role: str | None = None) -> str:
    base = [
        "Команды Smart Config:",
        "/start — получить код входа",
        "/me — мой профиль",
        "/servers — список серверов и ссылок",
        "/history — последние 25 запусков",
    ]
    if role in {"admin", "editor"}:
        base += [
            "",
            "Editor/Admin:",
            "/add_server Название",
            "/rename_server server_id Новое название",
            "/delete_server server_id",
            "/add_link server_id UK|PDD Название | https://url",
            "/edit_link link_id UK|PDD Название | https://url",
            "/delete_link link_id",
            "/check_link https://url",
            "/incidents — последние ошибки парсинга",
        ]
    if role == "admin":
        base += [
            "",
            "Admin:",
            "/add_user telegram_id role limit [username]",
            "/set_role telegram_id admin|editor|user",
            "/set_limit telegram_id число",
            "/ban_user telegram_id",
            "/unban_user telegram_id",
            "/users — список пользователей",
        ]
    return "\n".join(base)


@dp.message(Command("start"))
async def start(message: Message):
    async with SessionLocal() as session:
        user = await get_actor(session, message.from_user.id)
        if not user:
            await message.answer("Доступ не выдан. Напиши администратору свой Telegram ID: " + str(message.from_user.id))
            return
        user.username = message.from_user.username or user.username
        code = secrets.token_hex(3).upper()
        session.add(LoginCode(telegram_id=user.telegram_id, code=code, expires_at=datetime.utcnow() + timedelta(minutes=10)))
        await session.commit()
        await message.answer(f"Код входа в приложение: {code}\nКод действует 10 минут.\n\n" + help_text(user.role))


@dp.message(Command("help"))
async def help_cmd(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        await message.answer(help_text(actor.role if actor else None))


@dp.message(Command("me"))
async def me(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not actor:
            await message.answer("Доступ не выдан.")
            return
        await message.answer(
            f"ID: {actor.id}\nTelegram ID: {actor.telegram_id}\nРоль: {actor.role}\nАктивен: {'да' if actor.is_active else 'нет'}\nЛимит: {actor.daily_limit}/день"
        )


@dp.message(Command("add_user"))
async def add_user(message: Message):
    parts = message.text.split(maxsplit=4)
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_admin(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) < 2:
            await message.answer("Формат: /add_user telegram_id [admin|editor|user] [limit] [username]")
            return
        try:
            telegram_id = int(parts[1])
            role = parts[2] if len(parts) > 2 else "user"
            limit = int(parts[3]) if len(parts) > 3 else settings.daily_limit_default
            username = parts[4].strip() if len(parts) > 4 else None
        except ValueError:
            await message.answer("telegram_id и limit должны быть числами")
            return
        if role not in VALID_ROLES:
            await message.answer("Роль должна быть admin/editor/user")
            return
        user = await session.scalar(select(User).where(User.telegram_id == telegram_id))
        if not user:
            user = User(telegram_id=telegram_id, role=role, daily_limit=limit, username=username)
            session.add(user)
        else:
            user.role = role
            user.daily_limit = limit
            user.is_active = True
            if username:
                user.username = username
        await session.commit()
        await message.answer("✅ Пользователь сохранён")


@dp.message(Command("set_role"))
async def set_role(message: Message):
    parts = message.text.split()
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_admin(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) != 3 or parts[2] not in VALID_ROLES:
            await message.answer("Формат: /set_role telegram_id admin|editor|user")
            return
        user = await session.scalar(select(User).where(User.telegram_id == int(parts[1])))
        if not user:
            await message.answer("Пользователь не найден")
            return
        user.role = parts[2]
        await session.commit()
        await message.answer("✅ Роль обновлена")


@dp.message(Command("set_limit"))
async def set_limit(message: Message):
    parts = message.text.split()
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_admin(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) != 3:
            await message.answer("Формат: /set_limit telegram_id число")
            return
        user = await session.scalar(select(User).where(User.telegram_id == int(parts[1])))
        if not user:
            await message.answer("Пользователь не найден")
            return
        user.daily_limit = int(parts[2])
        await session.commit()
        await message.answer("✅ Лимит обновлён")


@dp.message(Command("ban_user", "unban_user"))
async def ban_user(message: Message):
    parts = message.text.split()
    cmd = parts[0].lstrip("/")
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_admin(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) < 2:
            await message.answer(f"Формат: /{cmd} telegram_id")
            return
        user = await session.scalar(select(User).where(User.telegram_id == int(parts[1])))
        if user:
            user.is_active = cmd == "unban_user"
            await session.commit()
        await message.answer("✅ Готово")


@dp.message(Command("users"))
async def users(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_admin(actor):
            await message.answer("Недостаточно прав.")
            return
        rows = (await session.execute(select(User).order_by(User.id))).scalars().all()
        lines = ["👥 Пользователи:"]
        for u in rows:
            lines.append(f"{u.id}. {u.telegram_id} @{u.username or '-'} — {u.role}, лимит {u.daily_limit}, {'active' if u.is_active else 'banned'}")
        await message.answer("\n".join(lines)[:3900])


@dp.message(Command("servers"))
async def servers(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not actor:
            await message.answer("Доступ не выдан.")
            return
        rows = (await session.execute(select(Server).options(selectinload(Server.links)).order_by(Server.name))).scalars().unique().all()
        if not rows:
            await message.answer("Серверов пока нет. Editor/Admin: /add_server Название")
            return
        chunks = []
        lines = ["🖥 Серверы:"]
        for s in rows:
            lines.append(f"\n{s.id}. {s.name} {'✅' if s.is_active else '🚫'}")
            for l in sorted(s.links, key=lambda x: (x.type, x.priority, x.id)):
                lines.append(f"  {l.id}. {l.type} — {l.title or '-'} — {'on' if l.is_active else 'off'}\n     {l.url}")
            text = "\n".join(lines)
            if len(text) > 3400:
                chunks.append(text)
                lines = []
        if lines:
            chunks.append("\n".join(lines))
        for chunk in chunks:
            await message.answer(chunk[:3900])


@dp.message(Command("add_server"))
async def add_server(message: Message):
    name = message.text.removeprefix("/add_server").strip()
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        if not name:
            await message.answer("Формат: /add_server Название")
            return
        server = Server(name=name)
        session.add(server)
        await session.commit()
        await message.answer(f"✅ Сервер добавлен: {server.id}. {server.name}")


@dp.message(Command("rename_server"))
async def rename_server(message: Message):
    parts = message.text.split(maxsplit=2)
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) < 3:
            await message.answer("Формат: /rename_server server_id Новое название")
            return
        server = await session.get(Server, int(parts[1]))
        if not server:
            await message.answer("Сервер не найден")
            return
        server.name = parts[2].strip()
        await session.commit()
        await message.answer("✅ Сервер переименован")


@dp.message(Command("delete_server"))
async def delete_server(message: Message):
    parts = message.text.split()
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) != 2:
            await message.answer("Формат: /delete_server server_id")
            return
        server = await session.get(Server, int(parts[1]))
        if server:
            await session.delete(server)
            await session.commit()
        await message.answer("✅ Сервер удалён")


def parse_link_payload(text: str, edit: bool = False):
    # /add_link server_id UK Title | url
    parts = text.split(maxsplit=3 if not edit else 3)
    min_len = 4
    if len(parts) < min_len:
        return None
    entity_id = int(parts[1])
    link_type = parts[2].upper()
    rest = parts[3]
    if "|" not in rest:
        title = ""
        url = rest.strip()
    else:
        title, url = [x.strip() for x in rest.split("|", 1)]
    return entity_id, link_type, title, url


@dp.message(Command("add_link"))
async def add_link(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        parsed = parse_link_payload(message.text)
        if not parsed:
            await message.answer("Формат: /add_link server_id UK|PDD Название | https://url")
            return
        server_id, link_type, title, url = parsed
        if link_type not in {"UK", "PDD"}:
            await message.answer("Тип должен быть UK или PDD")
            return
        ok, msg = await validate_forum_url(url)
        if not ok:
            await message.answer(f"❌ Ссылка не сохранена: {msg}")
            return
        link = ServerLink(server_id=server_id, type=link_type, title=title, url=url)
        session.add(link)
        await session.commit()
        await message.answer(f"✅ Ссылка добавлена: {link.id}")


@dp.message(Command("edit_link"))
async def edit_link(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        parsed = parse_link_payload(message.text, edit=True)
        if not parsed:
            await message.answer("Формат: /edit_link link_id UK|PDD Название | https://url")
            return
        link_id, link_type, title, url = parsed
        link = await session.get(ServerLink, link_id)
        if not link:
            await message.answer("Ссылка не найдена")
            return
        ok, msg = await validate_forum_url(url)
        if not ok:
            await message.answer(f"❌ Ссылка не сохранена: {msg}")
            return
        link.type = link_type
        link.title = title
        link.url = url
        await session.commit()
        await message.answer("✅ Ссылка обновлена")


@dp.message(Command("delete_link"))
async def delete_link(message: Message):
    parts = message.text.split()
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        if len(parts) != 2:
            await message.answer("Формат: /delete_link link_id")
            return
        link = await session.get(ServerLink, int(parts[1]))
        if link:
            await session.delete(link)
            await session.commit()
        await message.answer("✅ Ссылка удалена")


@dp.message(Command("check_link"))
async def check_link(message: Message):
    url = message.text.removeprefix("/check_link").strip()
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
    if not url:
        await message.answer("Формат: /check_link https://url")
        return
    ok, msg = await validate_forum_url(url)
    await message.answer(("✅ " if ok else "❌ ") + msg)


@dp.message(Command("history"))
async def history(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not actor:
            await message.answer("Доступ не выдан.")
            return
        rows = (await session.execute(select(CheckRun).options(selectinload(CheckRun.user), selectinload(CheckRun.server)).order_by(CheckRun.created_at.desc()).limit(25))).scalars().all()
        lines = ["📊 Последние 25 запусков:"]
        for r in rows:
            user = r.user.username or str(r.user.telegram_id) if r.user else "unknown"
            server = r.server.name if r.server else str(r.server_id or "local")
            lines.append(f"{r.created_at:%d.%m %H:%M} | {user} | {server} | {r.mode} | {r.ai_provider} | {r.status} | лимит {'да' if r.limit_charged else 'нет'}")
        await message.answer("\n".join(lines)[:3900])


@dp.message(Command("incidents"))
async def incidents(message: Message):
    async with SessionLocal() as session:
        actor = await get_actor(session, message.from_user.id)
        if not is_editor(actor):
            await message.answer("Недостаточно прав.")
            return
        rows = (await session.execute(select(Incident).order_by(Incident.created_at.desc()).limit(25))).scalars().all()
        lines = ["⚠️ Инциденты:"]
        for i in rows:
            lines.append(f"{i.id}. {i.created_at:%d.%m %H:%M} server={i.server_id or '-'} link={i.link_id or '-'} {'✅' if i.is_resolved else '❌'}\n{i.message}")
        await message.answer("\n".join(lines)[:3900])


async def notify_editors(text: str) -> None:
    async with SessionLocal() as session:
        users = (await session.execute(select(User).where(User.role.in_(["admin", "editor"]), User.is_active == True))).scalars().all()  # noqa: E712
        for user in users:
            try:
                await bot.send_message(user.telegram_id, text)
            except Exception:
                pass


async def main() -> None:
    await init_db()
    await dp.start_polling(bot)


if __name__ == "__main__":
    asyncio.run(main())

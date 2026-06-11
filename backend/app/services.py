from datetime import datetime
from zoneinfo import ZoneInfo

import httpx
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from .models import DailyUsage, Incident, Server, ServerLink, User
from .settings import settings


VALID_ROLES = {"admin", "editor", "user"}
VALID_LINK_TYPES = {"UK", "PDD"}
VALID_RUN_MODES = {"UK", "PDD", "BOTH"}
VALID_STATUSES = {"success", "failed", "cancelled"}


def date_key() -> str:
    return datetime.now(ZoneInfo(settings.server_tz)).strftime("%Y-%m-%d")


async def usage_for_today(session: AsyncSession, user: User) -> DailyUsage | None:
    return await session.scalar(
        select(DailyUsage).where(DailyUsage.user_id == user.id, DailyUsage.date_key == date_key())
    )


async def can_run(session: AsyncSession, user: User, ai_provider: str) -> tuple[bool, int, int]:
    if ai_provider.lower() == "gemini":
        return True, 999999, 0
    usage = await usage_for_today(session, user)
    used = usage.used if usage else 0
    left = max(user.daily_limit - used, 0)
    return used < user.daily_limit, left, used


async def charge_success(session: AsyncSession, user: User, ai_provider: str) -> bool:
    if ai_provider.lower() == "gemini":
        return False
    usage = await usage_for_today(session, user)
    if not usage:
        usage = DailyUsage(user_id=user.id, date_key=date_key(), used=0)
        session.add(usage)
    if usage.used >= user.daily_limit:
        return False
    usage.used += 1
    await session.flush()
    return True


async def get_servers_with_links(session: AsyncSession, active_only: bool = True) -> list[Server]:
    stmt = select(Server).options(selectinload(Server.links)).order_by(Server.name)
    if active_only:
        stmt = stmt.where(Server.is_active == True)  # noqa: E712
    return (await session.execute(stmt)).scalars().unique().all()


async def validate_forum_url(url: str) -> tuple[bool, str]:
    if not url.startswith(("http://", "https://")):
        return False, "Ссылка должна начинаться с http:// или https://"
    try:
        async with httpx.AsyncClient(timeout=15, follow_redirects=True) as client:
            resp = await client.get(url, headers={"User-Agent": "SmartConfigEditor/1.0"})
    except Exception as exc:
        return False, f"Не удалось открыть ссылку: {exc}"

    if resp.status_code >= 400:
        return False, f"HTTP {resp.status_code}"
    text = resp.text.lower()
    forum_markers = ["forum", "thread", "тема", "сообщ", "xenforo", "ipsfocus", "ips"]
    doc_markers = ["ук", "пдд", "статья", "штраф", "правил", "кодекс"]
    if not any(m in text for m in forum_markers) and not any(m in text for m in doc_markers):
        return False, "Страница не похожа на форум/документ с правилами"
    if len(resp.text.strip()) < 200:
        return False, "Страница слишком короткая"
    return True, "Ссылка подходит"


async def create_incident(
    session: AsyncSession,
    *,
    server_id: int | None,
    link_id: int | None,
    user_id: int | None,
    message: str,
    incident_type: str = "parse_error",
) -> Incident:
    incident = Incident(
        server_id=server_id,
        link_id=link_id,
        user_id=user_id,
        type=incident_type,
        message=message,
    )
    session.add(incident)
    await session.flush()
    return incident


def server_to_dict(server: Server) -> dict:
    return {
        "id": server.id,
        "name": server.name,
        "is_active": server.is_active,
        "links": [
            {
                "id": link.id,
                "server_id": link.server_id,
                "type": link.type,
                "title": link.title,
                "url": link.url,
                "priority": link.priority,
                "is_active": link.is_active,
            }
            for link in sorted(server.links, key=lambda l: (l.type, l.priority, l.id))
            if link.is_active
        ],
    }

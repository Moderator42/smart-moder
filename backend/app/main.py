from datetime import datetime
from typing import Literal

import httpx
from fastapi import Depends, FastAPI, HTTPException, status
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from .auth import create_token, get_current_user, require_roles
from .db import get_session, init_db
from .models import CheckRun, DailyUsage, Incident, LoginCode, Server, ServerLink, User
from .services import (
    VALID_LINK_TYPES,
    VALID_ROLES,
    VALID_RUN_MODES,
    VALID_STATUSES,
    can_run,
    charge_success,
    create_incident,
    date_key,
    get_servers_with_links,
    server_to_dict,
    validate_forum_url,
)
from .settings import settings

app = FastAPI(title="Smart Config Team API")


class LoginByCodeIn(BaseModel):
    code: str


class ServerIn(BaseModel):
    name: str = Field(min_length=1, max_length=128)


class ServerPatchIn(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=128)
    is_active: bool | None = None


class LinkIn(BaseModel):
    type: Literal["UK", "PDD"]
    title: str = ""
    url: str
    priority: int = 100
    is_active: bool = True
    validate_before_save: bool = False


class LinkPatchIn(BaseModel):
    type: Literal["UK", "PDD"] | None = None
    title: str | None = None
    url: str | None = None
    priority: int | None = None
    is_active: bool | None = None
    validate_before_save: bool = False


class RunStartIn(BaseModel):
    server_id: int | None = None
    mode: Literal["UK", "PDD", "BOTH"]
    ai_provider: Literal["openai", "gemini"]


class RunFinishIn(RunStartIn):
    status: Literal["success", "failed", "cancelled"]
    file_saved: bool = False
    message: str = ""


class IncidentIn(BaseModel):
    server_id: int | None = None
    link_id: int | None = None
    message: str
    type: str = "parse_error"


class OpenAiGenerateIn(BaseModel):
    mode: Literal["UK", "PDD"]
    forum_text: str
    base_json: str
    model: str = "gpt-4.1-mini"


@app.on_event("startup")
async def startup() -> None:
    await init_db()


@app.get("/health")
async def health():
    return {"ok": True, "server_time": datetime.utcnow().isoformat()}


@app.post("/auth/telegram-code")
async def login_by_code(payload: LoginByCodeIn, session: AsyncSession = Depends(get_session)):
    code_text = payload.code.strip().upper()
    code = await session.scalar(
        select(LoginCode).where(LoginCode.code == code_text, LoginCode.used == False)  # noqa: E712
    )
    if not code or code.expires_at < datetime.utcnow():
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Invalid or expired code")
    user = await session.scalar(select(User).where(User.telegram_id == code.telegram_id))
    if not user or not user.is_active:
        raise HTTPException(status.HTTP_403_FORBIDDEN, "User is not allowed")
    code.used = True
    await session.commit()
    return {
        "token": create_token(user.id, user.role),
        "user": {
            "id": user.id,
            "telegram_id": user.telegram_id,
            "username": user.username,
            "role": user.role,
            "daily_limit": user.daily_limit,
        },
    }


@app.get("/auth/me")
async def me(user: User = Depends(get_current_user), session: AsyncSession = Depends(get_session)):
    usage = await session.scalar(select(DailyUsage).where(DailyUsage.user_id == user.id, DailyUsage.date_key == date_key()))
    used = usage.used if usage else 0
    return {
        "id": user.id,
        "telegram_id": user.telegram_id,
        "username": user.username,
        "role": user.role,
        "daily_limit": user.daily_limit,
        "used_today": used,
        "left_today": max(user.daily_limit - used, 0),
        "date_key": date_key(),
    }


@app.get("/servers")
async def list_servers(
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(get_session),
):
    return [server_to_dict(s) for s in await get_servers_with_links(session, active_only=user.role == "user")]


@app.post("/servers")
async def create_server(
    payload: ServerIn,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    existing = await session.scalar(select(Server).where(Server.name == payload.name.strip()))
    if existing:
        raise HTTPException(409, "Сервер с таким названием уже есть")
    server = Server(name=payload.name.strip())
    session.add(server)
    await session.commit()
    await session.refresh(server)
    return {"id": server.id, "name": server.name, "is_active": server.is_active, "links": []}


@app.patch("/servers/{server_id}")
async def update_server(
    server_id: int,
    payload: ServerPatchIn,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    server = await session.get(Server, server_id, options=[selectinload(Server.links)])
    if not server:
        raise HTTPException(404, "Сервер не найден")
    if payload.name is not None:
        server.name = payload.name.strip()
    if payload.is_active is not None:
        server.is_active = payload.is_active
    await session.commit()
    return server_to_dict(server)


@app.delete("/servers/{server_id}")
async def delete_server(
    server_id: int,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    server = await session.get(Server, server_id)
    if not server:
        return {"ok": True}
    await session.delete(server)
    await session.commit()
    return {"ok": True}


@app.post("/servers/{server_id}/links")
async def create_link(
    server_id: int,
    payload: LinkIn,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    server = await session.get(Server, server_id)
    if not server:
        raise HTTPException(404, "Сервер не найден")
    if payload.validate_before_save:
        ok, msg = await validate_forum_url(payload.url)
        if not ok:
            raise HTTPException(400, msg)
    link = ServerLink(
        server_id=server_id,
        type=payload.type,
        title=payload.title.strip(),
        url=payload.url.strip(),
        priority=payload.priority,
        is_active=payload.is_active,
    )
    session.add(link)
    await session.commit()
    await session.refresh(link)
    return {"id": link.id, "server_id": link.server_id, "type": link.type, "title": link.title, "url": link.url, "priority": link.priority, "is_active": link.is_active}


@app.patch("/links/{link_id}")
async def update_link(
    link_id: int,
    payload: LinkPatchIn,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    link = await session.get(ServerLink, link_id)
    if not link:
        raise HTTPException(404, "Ссылка не найдена")
    if payload.url is not None and payload.validate_before_save:
        ok, msg = await validate_forum_url(payload.url)
        if not ok:
            raise HTTPException(400, msg)
    if payload.type is not None:
        link.type = payload.type
    if payload.title is not None:
        link.title = payload.title.strip()
    if payload.url is not None:
        link.url = payload.url.strip()
    if payload.priority is not None:
        link.priority = payload.priority
    if payload.is_active is not None:
        link.is_active = payload.is_active
    await session.commit()
    return {"id": link.id, "server_id": link.server_id, "type": link.type, "title": link.title, "url": link.url, "priority": link.priority, "is_active": link.is_active}


@app.delete("/links/{link_id}")
async def delete_link(
    link_id: int,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    link = await session.get(ServerLink, link_id)
    if link:
        await session.delete(link)
        await session.commit()
    return {"ok": True}


@app.post("/links/validate")
async def validate_link(payload: LinkIn, _: User = Depends(require_roles("admin", "editor"))):
    ok, msg = await validate_forum_url(payload.url)
    return {"ok": ok, "message": msg}


@app.post("/runs/can-start")
async def runs_can_start(
    payload: RunStartIn,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(get_session),
):
    allowed, left, used = await can_run(session, user, payload.ai_provider)
    return {"allowed": allowed, "left": left, "used": used, "daily_limit": user.daily_limit}


@app.post("/runs/finish")
async def runs_finish(
    payload: RunFinishIn,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(get_session),
):
    charged = False
    if payload.status == "success" and payload.file_saved:
        charged = await charge_success(session, user, payload.ai_provider)
    run = CheckRun(
        user_id=user.id,
        server_id=payload.server_id,
        mode=payload.mode,
        ai_provider=payload.ai_provider,
        status=payload.status,
        limit_charged=charged,
        file_saved=payload.file_saved,
        message=payload.message,
    )
    session.add(run)
    await session.commit()
    return {"ok": True, "limit_charged": charged}


@app.get("/history")
async def history(user: User = Depends(get_current_user), session: AsyncSession = Depends(get_session)):
    stmt = (
        select(CheckRun)
        .options(selectinload(CheckRun.user), selectinload(CheckRun.server))
        .order_by(CheckRun.created_at.desc())
        .limit(25)
    )
    rows = (await session.execute(stmt)).scalars().all()
    return [
        {
            "id": row.id,
            "created_at": int(row.created_at.timestamp()),
            "user": row.user.username or str(row.user.telegram_id) if row.user else "unknown",
            "server": row.server.name if row.server else str(row.server_id or "local"),
            "server_id": row.server_id,
            "mode": row.mode,
            "ai_provider": row.ai_provider,
            "status": row.status,
            "limit_charged": row.limit_charged,
            "file_saved": row.file_saved,
            "message": row.message,
        }
        for row in rows
    ]


@app.post("/incidents")
async def add_incident(
    payload: IncidentIn,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(get_session),
):
    incident = await create_incident(
        session,
        server_id=payload.server_id,
        link_id=payload.link_id,
        user_id=user.id,
        incident_type=payload.type,
        message=payload.message,
    )
    await session.commit()
    try:
        from .bot import notify_editors

        await notify_editors(
            f"⚠️ Ошибка парсинга\n\n"
            f"Пользователь: {user.username or user.telegram_id}\n"
            f"Сервер ID: {payload.server_id or '-'}\n"
            f"Ссылка ID: {payload.link_id or '-'}\n"
            f"Причина: {payload.message}"
        )
    except Exception:
        pass
    return {"ok": True, "incident_id": incident.id}


@app.get("/incidents")
async def list_incidents(
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    rows = (await session.execute(select(Incident).order_by(Incident.created_at.desc()).limit(25))).scalars().all()
    return [
        {
            "id": r.id,
            "server_id": r.server_id,
            "link_id": r.link_id,
            "user_id": r.user_id,
            "type": r.type,
            "message": r.message,
            "is_resolved": r.is_resolved,
            "created_at": int(r.created_at.timestamp()),
        }
        for r in rows
    ]


@app.patch("/incidents/{incident_id}/resolve")
async def resolve_incident(
    incident_id: int,
    _: User = Depends(require_roles("admin", "editor")),
    session: AsyncSession = Depends(get_session),
):
    incident = await session.get(Incident, incident_id)
    if not incident:
        raise HTTPException(404, "Инцидент не найден")
    incident.is_resolved = True
    await session.commit()
    return {"ok": True}


@app.post("/ai/openai/generate")
async def openai_generate(
    payload: OpenAiGenerateIn,
    user: User = Depends(get_current_user),
):
    if not settings.openai_api_key:
        raise HTTPException(500, "OPENAI_API_KEY не задан на backend")
    if len(payload.forum_text.strip()) < 200:
        raise HTTPException(400, "forum_text слишком короткий")

    system_prompt = "Ты обновляешь JSON для SmartUK/SmartPDD. Верни только валидный JSON-массив без markdown."
    user_prompt = (
        f"Режим: {payload.mode}\n\n"
        f"Текст форума:\n{payload.forum_text}\n\n"
        f"Текущий JSON:\n{payload.base_json}\n\n"
        "Сохрани структуру массива глав и статей. Обнови только по актуальному форуму."
    )
    async with httpx.AsyncClient(timeout=120) as client:
        resp = await client.post(
            "https://api.openai.com/v1/chat/completions",
            headers={"Authorization": f"Bearer {settings.openai_api_key}"},
            json={
                "model": payload.model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_prompt},
                ],
                "temperature": 0.1,
            },
        )
    if resp.status_code >= 400:
        raise HTTPException(resp.status_code, resp.text[:500])
    data = resp.json()
    return {"text": data["choices"][0]["message"]["content"]}

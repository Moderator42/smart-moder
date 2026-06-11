from sqlalchemy import select
from sqlalchemy.ext.asyncio import async_sessionmaker, create_async_engine, AsyncSession
from .settings import settings
from .models import Base, User

engine = create_async_engine(settings.database_url, pool_pre_ping=True)
SessionLocal = async_sessionmaker(engine, expire_on_commit=False)

async def get_session() -> AsyncSession:
    async with SessionLocal() as session:
        yield session

async def init_db() -> None:
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    # Bootstrap first admins from .env so the bot is not locked on a fresh DB.
    ids = [x.strip() for x in settings.admin_telegram_ids.split(",") if x.strip()]
    if not ids:
        return
    async with SessionLocal() as session:
        changed = False
        for raw_id in ids:
            try:
                telegram_id = int(raw_id)
            except ValueError:
                continue
            user = await session.scalar(select(User).where(User.telegram_id == telegram_id))
            if not user:
                session.add(User(telegram_id=telegram_id, role="admin", daily_limit=settings.daily_limit_default))
                changed = True
            elif user.role != "admin" or not user.is_active:
                user.role = "admin"
                user.is_active = True
                changed = True
        if changed:
            await session.commit()

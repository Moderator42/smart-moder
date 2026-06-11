from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    database_url: str
    redis_url: str = "redis://localhost:6379/0"
    jwt_secret: str
    telegram_bot_token: str
    openai_api_key: str = ""
    server_tz: str = "Europe/Berlin"
    daily_limit_default: int = 4
    admin_telegram_ids: str = ""  # comma-separated ids for first bootstrap admins

    class Config:
        env_file = ".env"

settings = Settings()  # type: ignore[call-arg]

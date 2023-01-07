import sentry_sdk
from fastapi import FastAPI

from app.views import router
from core.config import env_config
from core.db import database

sentry_sdk.init(
    env_config.SENTRY_DSN,
)


def start_app() -> FastAPI:
    app = FastAPI()

    app.include_router(router)

    app.state.database = database

    @app.on_event("startup")
    async def startup() -> None:
        database_ = app.state.database
        if not database_.is_connected:
            await database_.connect()

    @app.on_event("shutdown")
    async def shutdown() -> None:
        database_ = app.state.database
        if database_.is_connected:
            await database_.disconnect()

    return app

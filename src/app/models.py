from datetime import datetime
from enum import Enum

import ormar

from core.db import metadata, database


class BaseMeta(ormar.ModelMeta):
    metadata = metadata
    database = database


class Statuses(str, Enum):
    pending = "pending"
    approved = "approved"
    blocked = "blocked"


class CachePrivileges(str, Enum):
    original = "original"
    buffer = "buffer"
    no_cache = "no_cache"


class Service(ormar.Model):
    class Meta(BaseMeta):
        tablename = "services"

    id: int = ormar.Integer(primary_key=True)  # type: ignore
    token: str = ormar.String(max_length=128, unique=True)  # type: ignore
    username: str = ormar.String(max_length=64, default="")  # type: ignore
    user: int = ormar.BigInteger()  # type: ignore
    status: str = ormar.String(
        max_length=12, choices=list(Statuses), default=Statuses.pending
    )  # type: ignore
    cache: str = ormar.String(
        max_length=12, choices=list(CachePrivileges), default=CachePrivileges.no_cache
    )  # type: ignore
    created_time = ormar.DateTime(timezone=True, default=datetime.now)

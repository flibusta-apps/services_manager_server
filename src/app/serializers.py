from datetime import datetime
from typing import Optional

from pydantic import BaseModel, constr

from app.models import Statuses, CachePrivileges


class ServiceCreate(BaseModel):
    token: constr(max_length=128)  # type: ignore
    user: Optional[str] = None
    username: constr(max_length=64)  # type: ignore
    status: Statuses
    cache: CachePrivileges


class ServiceDetail(BaseModel):
    id: int
    token: str
    username: Optional[str]
    user: str
    status: str
    cache: str
    created_time: datetime

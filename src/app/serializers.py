from datetime import datetime

from pydantic import BaseModel, constr

from app.models import Statuses, CachePrivileges


class ServiceCreate(BaseModel):
    token: constr(max_length=128)  # type: ignore
    user: str
    status: Statuses
    cache: CachePrivileges


class ServiceDetail(BaseModel):
    id: int
    token: str
    user: str
    status: str
    cache: str
    created_time: datetime

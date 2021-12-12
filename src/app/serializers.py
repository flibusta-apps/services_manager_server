from datetime import datetime

from pydantic import BaseModel, constr

from app.models import Statuses


class ServiceCreate(BaseModel):
    token: constr(max_length=128)  # type: ignore
    user: str
    status: Statuses


class ServiceDetail(BaseModel):
    id: int
    token: str
    user: str
    status: str
    privileged: bool
    created_time: datetime

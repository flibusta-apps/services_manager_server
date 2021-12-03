from fastapi import APIRouter, HTTPException, status, Depends

from app.depends import check_token
from app.serializers import ServiceCreate, ServiceDetail
from app.models import Service


# TODO: add redis cache


router = APIRouter(
    dependencies=[Depends(check_token)]
)


@router.get("/", response_model=list[ServiceDetail])
async def get_services():
    return await Service.objects.all()


@router.get("/{id}", response_model=ServiceDetail)
async def get_service(id: int):
    service = await Service.objects.get_or_none(id=id)

    if service is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    
    return service


@router.post("/", response_model=ServiceDetail)
async def register_service(data: ServiceCreate):
    return await Service.objects.create(**data.dict())

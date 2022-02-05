from fastapi import APIRouter, HTTPException, status, Depends

from app.depends import check_token
from app.models import CachePrivileges, Service, Statuses
from app.serializers import ServiceCreate, ServiceDetail


# TODO: add redis cache


router = APIRouter(dependencies=[Depends(check_token)])


@router.get("/", response_model=list[ServiceDetail])
async def get_services():
    return await Service.objects.all()


@router.get("/healthcheck")
async def healthcheck():
    return "Ok!"


@router.get("/{id}", response_model=ServiceDetail)
async def get_service(id: int):
    service = await Service.objects.get_or_none(id=id)

    if service is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)

    return service


@router.post("/", response_model=ServiceDetail)
async def register_service(data: ServiceCreate):
    return await Service.objects.create(**data.dict())


@router.patch("/{id}/update_status", response_model=ServiceDetail)
async def update_service_state(id: int, new_status: Statuses):
    service = await Service.objects.get_or_none(id=id)

    if service is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)

    service.status = new_status

    await service.update(["status"])

    return service


@router.patch("/{id}/update_cache", response_model=ServiceDetail)
async def update_service_cache(id: int, new_cache: CachePrivileges):
    service = await Service.objects.get_or_none(id=id)

    if service is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)

    service.cache = new_cache

    await service.update(["cache"])

    return service

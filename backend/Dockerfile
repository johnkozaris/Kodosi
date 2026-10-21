FROM --platform=$BUILDPLATFORM mcr.microsoft.com/dotnet/sdk:10.0.402@sha256:747cbade5bfd918a175ac32f8f1d931edec2c7d0d5397a3de72eaef1ac1f66d2 AS build
WORKDIR /source
COPY backend/ backend/
RUN dotnet restore backend/src/Kodosi.Host/Kodosi.Host.csproj --locked-mode
RUN dotnet publish backend/src/Kodosi.Host/Kodosi.Host.csproj --configuration Release --no-restore --output /app/publish /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:10.0.13@sha256:8af87eba5e7cdafb1514d7388be287365da97c132509e84dfe252ffd552398ca AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/publish/ ./
USER $APP_UID
EXPOSE 8080
ENTRYPOINT ["dotnet", "Kodosi.Host.dll"]

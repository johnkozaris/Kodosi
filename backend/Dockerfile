FROM --platform=$BUILDPLATFORM mcr.microsoft.com/dotnet/sdk:11.0.100@sha256:65f3c7c6a25a239ab3e6719f2896147e7b68242ff2d8818dd70498bde6ef8bea AS build
WORKDIR /source
COPY backend/ backend/
RUN dotnet restore backend/src/Kodosi.Host/Kodosi.Host.csproj --locked-mode
RUN dotnet publish backend/src/Kodosi.Host/Kodosi.Host.csproj --configuration Release --no-restore --output /app/publish /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:11.0.0@sha256:bc0d6653979efba491ab3d6f589d691f96c309c3c3ef81325efa6978e7b7f242 AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/publish/ ./
USER $APP_UID
EXPOSE 8080 3478/udp
ENTRYPOINT ["dotnet", "Kodosi.Host.dll"]

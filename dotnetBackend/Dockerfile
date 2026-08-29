FROM mcr.microsoft.com/dotnet/sdk:10.0.400@sha256:e1ffd2a92ae84c1291bc1b6887501f8af98e6331e7af6d4c8d37168c5e87a64c AS build
WORKDIR /source
COPY dotnetBackend/ dotnetBackend/
COPY protocol/backend-api-authority.json protocol/backend-api-authority.json
RUN dotnet restore dotnetBackend/Kodosi.slnx --locked-mode
RUN dotnet publish dotnetBackend/src/Kodosi.Host/Kodosi.Host.csproj \
    --configuration Release \
    --no-restore \
    --output /app/publish \
    /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:10.0.11@sha256:a4556ed033fa96f984bb7a8d348851cb2d36b1281dd2420070045f664fbb5f94 AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /home/app/.aspnet/DataProtection-Keys \
    && chown -R "$APP_UID:$APP_UID" /home/app/.aspnet
WORKDIR /app
COPY --from=build /app/publish/ ./
USER $APP_UID
EXPOSE 8080
ENTRYPOINT ["dotnet", "Kodosi.Host.dll"]
